//! What a player runs: one motion, or a sequence of them.
//!
//! A closed enum rather than `Box<dyn>` — there are exactly two kinds and
//! both are cheap to clone, so dispatch costs a branch and sampling stays
//! allocation-free.

use super::{Frame, Motion, Sequence};

#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
  Motion(Motion),
  Sequence(Sequence),
}

impl Default for Clip {
  fn default() -> Self {
    Clip::Motion(Motion::new())
  }
}

impl Clip {
  /// One pass, in milliseconds.
  pub fn iteration_ms(&self) -> f32 {
    match self {
      Clip::Motion(motion) => motion.iteration_ms(),
      Clip::Sequence(sequence) => sequence.iteration_ms(),
    }
  }

  /// Every pass, or `f32::INFINITY` when it never ends.
  pub fn total_ms(&self) -> f32 {
    match self {
      Clip::Motion(motion) => motion.total_ms(),
      Clip::Sequence(sequence) => sequence.total_ms(),
    }
  }

  /// Whether it runs forever — the thing a player can never wait out.
  pub fn is_endless(&self) -> bool {
    !self.total_ms().is_finite()
  }

  /// Whether every other pass runs backwards.
  pub fn alternates(&self) -> bool {
    match self {
      Clip::Motion(motion) => motion.alternate,
      Clip::Sequence(sequence) => sequence.is_alternating(),
    }
  }

  pub fn sample(&self, t: f32) -> Frame {
    let mut frame = Frame::new();
    self.sample_into(t, &mut frame);
    frame
  }

  pub fn sample_into(&self, t: f32, frame: &mut Frame) {
    match self {
      Clip::Motion(motion) => motion.sample_into(t, frame),
      Clip::Sequence(sequence) => sequence.sample_into(t, frame),
    }
  }
}

impl From<Motion> for Clip {
  fn from(motion: Motion) -> Self {
    Clip::Motion(motion)
  }
}

impl From<Sequence> for Clip {
  fn from(sequence: Sequence) -> Self {
    Clip::Sequence(sequence)
  }
}
