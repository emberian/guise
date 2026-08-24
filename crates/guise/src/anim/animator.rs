//! `Animator` — a clip with a clock you can drive.
//!
//! gpui's own `with_animation` is fire-and-forget: it starts when the element
//! first lays out and there is no handle to pause, reverse, or scrub it. That
//! is the right thing for an entrance and useless for anything a user
//! controls, so `Animator` keeps the playback state in an entity instead.
//!
//! The clock is an *anchor*, not a tick: `time` is where the playhead was at
//! `anchor`, and everything else is derived from `Instant::now()`. Nothing
//! mutates per frame, so a paused animation costs nothing, seeking is one
//! assignment, and sampling is pure enough to unit-test without a window.
//!
//! Frames come from [`Animator::frame`], which also asks the window for the
//! next one while the clip is still running — that is the whole repaint loop.

use std::time::{Duration, Instant};

use gpui::{Context, EventEmitter, Task, Window};

use super::{Clip, Frame};

/// The ends of a playback run. There is no per-frame event: your `render`
/// already runs every frame, and reading [`Animator::frame`] there is the
/// same thing without the plumbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimatorEvent {
    /// Playback started (or restarted) from a stopped state.
    Begin,
    /// The playhead reached the end — or the start, when running reversed.
    Complete,
}

/// A clip plus a playhead.
pub struct Animator {
    clip: Clip,
    /// Playhead position in ms, correct as of `anchor`.
    time: f32,
    /// `Some` while running: the wall clock that `time` was measured at.
    anchor: Option<Instant>,
    speed: f32,
    reversed: bool,
    /// Bumped on every state change so a stale completion timer gives up.
    epoch: usize,
    completion: Option<Task<()>>,
}

impl EventEmitter<AnimatorEvent> for Animator {}

impl Animator {
    pub fn new(clip: impl Into<Clip>, _cx: &mut Context<Self>) -> Self {
        Animator {
            clip: clip.into(),
            time: 0.0,
            anchor: None,
            speed: 1.0,
            reversed: false,
            epoch: 0,
            completion: None,
        }
    }

    /// Start playing as soon as it is created.
    pub fn autoplay(mut self, cx: &mut Context<Self>) -> Self {
        self.play(cx);
        self
    }

    /// Run the clip backwards from the start.
    pub fn reversed(mut self, reversed: bool) -> Self {
        self.reversed = reversed;
        self.time = if reversed { self.total_ms() } else { 0.0 };
        self
    }

    pub fn clip(&self) -> &Clip {
        &self.clip
    }

    fn total_ms(&self) -> f32 {
        self.clip.total_ms()
    }

    /// Where the playhead is right now, in milliseconds.
    pub fn time(&self) -> f32 {
        self.time_at(Instant::now())
    }

    /// Where the playhead would be at `now`. The pure form — tests drive
    /// this instead of sleeping.
    pub fn time_at(&self, now: Instant) -> f32 {
        let Some(anchor) = self.anchor else {
            return self.time;
        };
        let elapsed = now.saturating_duration_since(anchor).as_secs_f32() * 1000.0 * self.speed;
        let raw = if self.reversed {
            self.time - elapsed
        } else {
            self.time + elapsed
        };
        raw.clamp(0.0, self.total_ms())
    }

    /// 0..=1 through the clip. Endless clips report their position within
    /// the current pass.
    pub fn progress(&self) -> f32 {
        self.clip.sample(self.time()).progress
    }

    /// Whether the clock is running. A clip that has played to its end
    /// reports `false` even before anything cleans up.
    pub fn is_playing(&self) -> bool {
        self.anchor.is_some() && !self.is_settled_at(Instant::now())
    }

    /// Whether the playhead has run out of road in the direction it is going.
    fn is_settled_at(&self, now: Instant) -> bool {
        if self.clip.is_endless() || self.speed <= 0.0 {
            return false;
        }
        let time = self.time_at(now);
        if self.reversed {
            time <= 0.0
        } else {
            time >= self.total_ms()
        }
    }

    /// The values for this instant, and a request for the next frame while
    /// the clip is still moving. Call it from `render`.
    pub fn frame(&self, window: &mut Window) -> Frame {
        let now = Instant::now();
        if self.anchor.is_some() && !self.is_settled_at(now) {
            window.request_animation_frame();
        }
        self.clip.sample(self.time_at(now))
    }

    /// The values at `now`, with no window and no repaint request.
    pub fn frame_at(&self, now: Instant) -> Frame {
        self.clip.sample(self.time_at(now))
    }

    pub fn play(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self.anchor.is_some() && !self.is_settled_at(now) {
            return;
        }
        // Replaying something that already finished starts it over, rather
        // than sitting on the end frame doing nothing.
        if self.is_settled_at(now) {
            self.time = if self.reversed { self.total_ms() } else { 0.0 };
        }
        self.anchor = Some(now);
        cx.emit(AnimatorEvent::Begin);
        self.reschedule(cx);
        cx.notify();
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if self.anchor.is_none() {
            return;
        }
        self.time = self.time_at(Instant::now());
        self.anchor = None;
        self.reschedule(cx);
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.is_playing() {
            self.pause(cx);
        } else {
            self.play(cx);
        }
    }

    /// Back to the beginning, playing.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        self.time = if self.reversed { self.total_ms() } else { 0.0 };
        self.anchor = Some(Instant::now());
        cx.emit(AnimatorEvent::Begin);
        self.reschedule(cx);
        cx.notify();
    }

    /// Back to the beginning, stopped.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.time = if self.reversed { self.total_ms() } else { 0.0 };
        self.anchor = None;
        self.reschedule(cx);
        cx.notify();
    }

    /// Move the playhead. Keeps playing if it was playing.
    pub fn seek(&mut self, ms: f32, cx: &mut Context<Self>) {
        self.time = ms.clamp(0.0, self.total_ms());
        if self.anchor.is_some() {
            self.anchor = Some(Instant::now());
        }
        self.reschedule(cx);
        cx.notify();
    }

    /// Move the playhead by fraction of the clip, 0..=1. Endless clips have
    /// no fraction to scrub, so this does nothing for them.
    pub fn seek_progress(&mut self, progress: f32, cx: &mut Context<Self>) {
        let total = self.total_ms();
        if total.is_finite() {
            self.seek(total * progress.clamp(0.0, 1.0), cx);
        }
    }

    /// Flip direction, leaving the playhead where it is.
    pub fn reverse(&mut self, cx: &mut Context<Self>) {
        self.time = self.time_at(Instant::now());
        self.reversed = !self.reversed;
        if self.anchor.is_some() {
            self.anchor = Some(Instant::now());
        }
        self.reschedule(cx);
        cx.notify();
    }

    pub fn is_reversed(&self) -> bool {
        self.reversed
    }

    pub fn set_speed(&mut self, speed: f32, cx: &mut Context<Self>) {
        self.time = self.time_at(Instant::now());
        self.speed = speed.max(0.0);
        if self.anchor.is_some() {
            self.anchor = Some(Instant::now());
        }
        self.reschedule(cx);
        cx.notify();
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Arm (or disarm) the one timer that fires `Complete`.
    ///
    /// One timer for the whole run, not a tick: the frame values come from
    /// the clock, so the only thing that needs waking up is the event. An
    /// endless clip never arms one.
    fn reschedule(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        self.completion = None;
        if self.anchor.is_none() || self.speed <= 0.0 || self.clip.is_endless() {
            return;
        }
        let remaining = if self.reversed {
            self.time
        } else {
            self.total_ms() - self.time
        };
        if remaining <= 0.0 {
            return;
        }
        let epoch = self.epoch;
        let wait = Duration::from_secs_f32((remaining / self.speed / 1000.0).max(0.0));
        self.completion = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |animator, cx| {
                if animator.epoch != epoch {
                    return;
                }
                animator.time = if animator.reversed {
                    0.0
                } else {
                    animator.total_ms()
                };
                animator.anchor = None;
                animator.completion = None;
                cx.emit(AnimatorEvent::Complete);
                cx.notify();
            })
            .ok();
        }));
    }
}
