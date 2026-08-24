//! Choreography across a list: the same motion, offset per element.
//!
//! One element is one clip, so staggering is not a timeline feature here —
//! it is a function from an index to a delay, which you fold into each
//! element's own [`Motion`](super::Motion). That keeps the N elements
//! independent (a list can grow or reorder mid-flight without restarting
//! anything) and makes the whole thing a pure calculation you can unit-test.
//!
//! ```ignore
//! let rise = Stagger::new(40.0).from(StaggerFrom::Center);
//! for (i, row) in rows.iter().enumerate() {
//!     Animated::new(("row", i))
//!         .motion(Motion::enter(TransitionKind::SlideUp).delay(rise.at(i, rows.len())))
//!         .child(row)
//! }
//! ```

use super::Easing;

/// Which element goes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StaggerFrom {
    #[default]
    First,
    Last,
    Center,
    /// A specific index leads and the rest ripple out from it.
    Index(usize),
}

/// Restrict a grid stagger to one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaggerAxis {
    X,
    Y,
}

/// Index-to-delay (or index-to-value) mapping for a list or a grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stagger {
    /// Milliseconds between neighbours.
    pub step: f32,
    /// Milliseconds added to everyone.
    pub start: f32,
    pub from: StaggerFrom,
    /// Columns and rows, when the elements are laid out as a grid.
    pub grid: Option<(usize, usize)>,
    /// With a grid, measure distance along one axis only.
    pub axis: Option<StaggerAxis>,
    /// Reshape the spacing — `Easing::In(Curve::Quad)` bunches the early
    /// elements together and spreads the late ones out.
    pub ease: Option<Easing>,
    pub reversed: bool,
}

impl Stagger {
    /// `step` milliseconds between neighbours.
    pub fn new(step: f32) -> Self {
        Stagger {
            step,
            start: 0.0,
            from: StaggerFrom::First,
            grid: None,
            axis: None,
            ease: None,
            reversed: false,
        }
    }

    pub fn start(mut self, ms: f32) -> Self {
        self.start = ms;
        self
    }

    pub fn from(mut self, from: StaggerFrom) -> Self {
        self.from = from;
        self
    }

    /// Treat the indices as a `columns × rows` grid in row-major order.
    pub fn grid(mut self, columns: usize, rows: usize) -> Self {
        self.grid = Some((columns.max(1), rows.max(1)));
        self
    }

    pub fn axis(mut self, axis: StaggerAxis) -> Self {
        self.axis = Some(axis);
        self
    }

    pub fn ease(mut self, easing: Easing) -> Self {
        self.ease = Some(easing);
        self
    }

    pub fn reversed(mut self, reversed: bool) -> Self {
        self.reversed = reversed;
        self
    }

    /// The delay for element `index` of `total`, in milliseconds.
    pub fn at(&self, index: usize, total: usize) -> f32 {
        self.start + self.weight(index, total) * self.furthest(total) * self.step
    }

    /// Spread a value instead of a delay: element 0 gets `from`, the
    /// furthest gets `to`, everyone else lands in between. anime.js's
    /// `stagger([a, b])`.
    pub fn value(&self, index: usize, total: usize, from: f32, to: f32) -> f32 {
        from + (to - from) * self.weight(index, total)
    }

    /// How long until the last element has started.
    pub fn span(&self, total: usize) -> f32 {
        self.start + self.furthest(total) * self.step
    }

    /// 0..=1: how far this index is from the leading one.
    fn weight(&self, index: usize, total: usize) -> f32 {
        let furthest = self.furthest(total);
        if furthest <= 0.0 {
            return 0.0;
        }
        let mut t = (self.distance(index, total) / furthest).clamp(0.0, 1.0);
        if self.reversed {
            t = 1.0 - t;
        }
        match self.ease {
            Some(easing) => easing.apply(t),
            None => t,
        }
    }

    /// Distance from the leading element, in element-widths.
    fn distance(&self, index: usize, total: usize) -> f32 {
        match self.grid {
            Some((columns, _)) => {
                let (x, y) = ((index % columns) as f32, (index / columns) as f32);
                let (ox, oy) = self.grid_origin(total);
                match self.axis {
                    Some(StaggerAxis::X) => (x - ox).abs(),
                    Some(StaggerAxis::Y) => (y - oy).abs(),
                    None => ((x - ox).powi(2) + (y - oy).powi(2)).sqrt(),
                }
            }
            None => {
                let last = total.saturating_sub(1) as f32;
                let origin = match self.from {
                    StaggerFrom::First => 0.0,
                    StaggerFrom::Last => last,
                    StaggerFrom::Center => last / 2.0,
                    StaggerFrom::Index(i) => i as f32,
                };
                (index as f32 - origin).abs()
            }
        }
    }

    fn grid_origin(&self, total: usize) -> (f32, f32) {
        let (columns, rows) = self.grid.unwrap_or((1, 1));
        let rows = rows.max(total.div_ceil(columns));
        let (last_x, last_y) = ((columns - 1) as f32, (rows.saturating_sub(1)) as f32);
        match self.from {
            StaggerFrom::First => (0.0, 0.0),
            StaggerFrom::Last => (last_x, last_y),
            StaggerFrom::Center => (last_x / 2.0, last_y / 2.0),
            StaggerFrom::Index(i) => ((i % columns) as f32, (i / columns) as f32),
        }
    }

    /// The largest distance any index reaches — what normalizes the weight
    /// so `ease` and `value` have a fixed range to work in.
    fn furthest(&self, total: usize) -> f32 {
        (0..total)
            .map(|i| self.distance(i, total))
            .fold(0.0_f32, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::ease::Curve;

    #[test]
    fn a_plain_stagger_steps_one_by_one() {
        let stagger = Stagger::new(50.0);
        assert_eq!(stagger.at(0, 4), 0.0);
        assert_eq!(stagger.at(1, 4), 50.0);
        assert_eq!(stagger.at(3, 4), 150.0);
        assert_eq!(stagger.span(4), 150.0);
    }

    #[test]
    fn start_shifts_everyone() {
        let stagger = Stagger::new(50.0).start(100.0);
        assert_eq!(stagger.at(0, 4), 100.0);
        assert_eq!(stagger.at(2, 4), 200.0);
    }

    #[test]
    fn from_last_reverses_the_order() {
        let stagger = Stagger::new(50.0).from(StaggerFrom::Last);
        assert_eq!(stagger.at(3, 4), 0.0);
        assert_eq!(stagger.at(0, 4), 150.0);
    }

    #[test]
    fn from_center_ripples_outward() {
        let stagger = Stagger::new(50.0).from(StaggerFrom::Center);
        // Five elements: the middle leads, the ends arrive together.
        assert_eq!(stagger.at(2, 5), 0.0);
        assert_eq!(stagger.at(0, 5), stagger.at(4, 5));
        assert!(stagger.at(1, 5) < stagger.at(0, 5));
    }

    #[test]
    fn a_named_index_leads() {
        let stagger = Stagger::new(10.0).from(StaggerFrom::Index(2));
        assert_eq!(stagger.at(2, 5), 0.0);
        assert_eq!(stagger.at(4, 5), 20.0);
    }

    #[test]
    fn a_grid_measures_in_two_dimensions() {
        let stagger = Stagger::new(100.0).grid(3, 2);
        // Row-major 3x2. Corner-to-corner is sqrt(2^2 + 1^2).
        assert_eq!(stagger.at(0, 6), 0.0);
        let far = stagger.at(5, 6);
        assert!((far - 100.0 * 5.0_f32.sqrt()).abs() < 1e-3, "{far}");
        // Same column, next row: distance 1.
        assert!((stagger.at(3, 6) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn an_axis_flattens_the_grid_to_one_direction() {
        let stagger = Stagger::new(100.0).grid(3, 2).axis(StaggerAxis::Y);
        assert_eq!(stagger.at(0, 6), stagger.at(2, 6), "same row, same delay");
        assert!(stagger.at(3, 6) > stagger.at(0, 6));
    }

    #[test]
    fn reversed_flips_the_weights() {
        let plain = Stagger::new(50.0);
        let flipped = Stagger::new(50.0).reversed(true);
        assert_eq!(flipped.at(0, 4), plain.at(3, 4));
        assert_eq!(flipped.at(3, 4), plain.at(0, 4));
    }

    #[test]
    fn easing_reshapes_the_spacing_without_moving_the_ends() {
        let eased = Stagger::new(50.0).ease(Easing::In(Curve::Quad));
        assert_eq!(eased.at(0, 5), 0.0);
        assert_eq!(eased.at(4, 5), 200.0);
        // Quadratic in: the early elements bunch up.
        assert!(eased.at(1, 5) < 50.0);
    }

    #[test]
    fn values_spread_across_a_range() {
        let stagger = Stagger::new(0.0);
        assert_eq!(stagger.value(0, 5, -100.0, 100.0), -100.0);
        assert_eq!(stagger.value(4, 5, -100.0, 100.0), 100.0);
        assert_eq!(stagger.value(2, 5, -100.0, 100.0), 0.0);
    }

    #[test]
    fn a_single_element_never_waits() {
        let stagger = Stagger::new(50.0).from(StaggerFrom::Center);
        assert_eq!(stagger.at(0, 1), 0.0);
        assert_eq!(stagger.span(1), 0.0);
        assert_eq!(stagger.at(0, 0), 0.0);
    }
}
