//! `Animated` — the element that plays a clip.
//!
//! Two shapes, one wrapper. Give it a [`Motion`] or a [`Sequence`] and it
//! plays once when the element mounts, no state anywhere: gpui's animation
//! element supplies the clock and the clip supplies the values, so a
//! staggered list of fifty rows is fifty stateless builders. Give it an
//! [`Animator`] instead and it renders that animator's playhead, asking the
//! window for the next frame while it is still moving.
//!
//! The values land on a wrapper `div` around your child. Offsets are relative
//! insets, so an animating element never pushes its neighbours around.

use gpui::prelude::*;
use gpui::{div, AnyElement, App, ElementId, Entity, IntoElement, Window};

use super::{Animator, Clip, Motion, Motioned, Sequence};
use crate::devtools::{Probed, ProbedAny};
use crate::transition::TransitionKind;

/// A child with a clip playing on it.
#[derive(IntoElement)]
pub struct Animated {
    id: ElementId,
    clip: Clip,
    animator: Option<Entity<Animator>>,
    child: Option<AnyElement>,
}

impl Animated {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Animated {
            id: id.into(),
            clip: Clip::default(),
            animator: None,
            child: None,
        }
    }

    /// Play this motion once when the element mounts.
    pub fn motion(mut self, motion: Motion) -> Self {
        self.clip = Clip::Motion(motion);
        self
    }

    /// Play this sequence once when the element mounts.
    pub fn sequence(mut self, sequence: Sequence) -> Self {
        self.clip = Clip::Sequence(sequence);
        self
    }

    pub fn clip(mut self, clip: impl Into<Clip>) -> Self {
        self.clip = clip.into();
        self
    }

    /// One of the stock entrances, at the default 200ms.
    pub fn enter(self, kind: TransitionKind) -> Self {
        self.motion(Motion::enter(kind))
    }

    /// Render an [`Animator`]'s playhead instead of a one-shot clip. The
    /// animator wins: whatever motion was set is ignored.
    pub fn animator(mut self, animator: &Entity<Animator>) -> Self {
        self.animator = Some(animator.clone());
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(child.into_any_element());
        self
    }
}

impl RenderOnce for Animated {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let child = self.child.unwrap_or_else(|| div().into_any_element());

        if let Some(animator) = self.animator {
            let frame = animator.read(cx).frame(window);
            let progress = frame.progress;
            return frame
                .apply(div())
                .child(child)
                .probe("Animated")
                .attr_with("progress", || format!("{progress:.2}"))
                .into_any_element();
        }

        div()
            .child(child)
            .animate(self.id, self.clip)
            .probe_any("Animated")
            .into_any_element()
    }
}
