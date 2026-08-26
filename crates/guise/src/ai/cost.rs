//! `AICost` — what this conversation has cost so far.
//!
//! Per-token prices are small enough to feel free and add up fast enough to
//! surprise, so a running total is worth the corner it takes. The arithmetic
//! is here rather than in the caller because getting it wrong by a factor of a
//! thousand is easy: prices are quoted per million tokens.

use gpui::prelude::*;
use gpui::{div, px, App, IntoElement, SharedString, Window};

use super::tokenmeter::compact;
use crate::devtools::Probed;
use crate::icon::{Icon, IconName};
use crate::theme::{theme, ColorName, Size};

/// Prices in US dollars per million tokens, the unit providers quote.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AIPricing {
  pub input_per_million: f64,
  pub output_per_million: f64,
  /// What a cache read costs, when the provider bills it separately.
  pub cache_read_per_million: f64,
}

impl AIPricing {
  pub fn new(input_per_million: f64, output_per_million: f64) -> Self {
    AIPricing {
      input_per_million,
      output_per_million,
      cache_read_per_million: 0.0,
    }
  }

  pub fn cache_read(mut self, per_million: f64) -> Self {
    self.cache_read_per_million = per_million;
    self
  }
}

/// A token tally for one request or a whole session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AIUsage {
  pub input: u64,
  pub output: u64,
  pub cache_read: u64,
}

impl AIUsage {
  pub fn new(input: u64, output: u64) -> Self {
    AIUsage {
      input,
      output,
      cache_read: 0,
    }
  }

  pub fn cache_read(mut self, tokens: u64) -> Self {
    self.cache_read = tokens;
    self
  }

  /// Every token that passed through, for a context-window meter.
  pub fn total(self) -> u64 {
    self
      .input
      .saturating_add(self.output)
      .saturating_add(self.cache_read)
  }

  /// What this usage costs in dollars at these prices.
  pub fn cost(self, pricing: AIPricing) -> f64 {
    let per = |tokens: u64, price: f64| tokens as f64 / 1_000_000.0 * price;
    per(self.input, pricing.input_per_million)
      + per(self.output, pricing.output_per_million)
      + per(self.cache_read, pricing.cache_read_per_million)
  }
}

/// Session totals are the sum of every request's usage. Saturating rather
/// than wrapping: a tally that silently rolls over to nearly zero is worse
/// than one that sticks at the ceiling.
impl std::ops::Add for AIUsage {
  type Output = AIUsage;

  fn add(self, other: AIUsage) -> AIUsage {
    AIUsage {
      input: self.input.saturating_add(other.input),
      output: self.output.saturating_add(other.output),
      cache_read: self.cache_read.saturating_add(other.cache_read),
    }
  }
}

impl std::iter::Sum for AIUsage {
  fn sum<I: Iterator<Item = AIUsage>>(iter: I) -> AIUsage {
    iter.fold(AIUsage::default(), |total, usage| total + usage)
  }
}

/// A running cost readout.
#[derive(IntoElement)]
pub struct AICost {
  usage: AIUsage,
  pricing: AIPricing,
  label: Option<SharedString>,
  size: Size,
  /// Show the input/output split under the total.
  breakdown: bool,
}

impl AICost {
  pub fn new(usage: AIUsage, pricing: AIPricing) -> Self {
    AICost {
      usage,
      pricing,
      label: None,
      size: Size::Xs,
      breakdown: false,
    }
  }

  pub fn label(mut self, label: impl Into<SharedString>) -> Self {
    self.label = Some(label.into());
    self
  }

  pub fn size(mut self, size: Size) -> Self {
    self.size = size;
    self
  }

  /// Break the total down by token kind.
  pub fn breakdown(mut self, breakdown: bool) -> Self {
    self.breakdown = breakdown;
    self
  }

  /// The total in dollars.
  pub fn total(&self) -> f64 {
    self.usage.cost(self.pricing)
  }
}

/// Money, at a precision that suits the amount: fractions of a cent still
/// need to be legible, dollars don't need six decimal places.
pub fn format_cost(dollars: f64) -> String {
  if !dollars.is_finite() || dollars <= 0.0 {
    return "$0.00".to_string();
  }
  if dollars < 0.01 {
    format!("${dollars:.4}")
  } else if dollars < 1.0 {
    format!("${dollars:.3}")
  } else {
    format!("${dollars:.2}")
  }
}

impl RenderOnce for AICost {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let font = t.font_size(self.size);
    let dimmed = t.dimmed().hsla();
    let text_color = t.text().hsla();
    let total = self.total();

    div()
      .flex()
      .flex_col()
      .gap(px(2.0))
      .text_size(px(font))
      .child(
        div()
          .flex()
          .items_center()
          .gap(px(6.0))
          .text_color(dimmed)
          .child(
            Icon::new(IconName::Coins)
              .size(Size::Xs)
              .color(ColorName::Gray),
          )
          .children(self.label)
          .child(
            div()
              .text_color(text_color)
              .child(SharedString::from(format_cost(total))),
          ),
      )
      .when(self.breakdown, |column| {
        column.child(
          div()
            .flex()
            .gap(px(10.0))
            .text_color(dimmed)
            .child(SharedString::from(format!(
              "in {}",
              compact(self.usage.input)
            )))
            .child(SharedString::from(format!(
              "out {}",
              compact(self.usage.output)
            )))
            .when(self.usage.cache_read > 0, |row| {
              row.child(SharedString::from(format!(
                "cache {}",
                compact(self.usage.cache_read)
              )))
            }),
        )
      })
      .probe("AICost")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cost_is_per_million_not_per_token() {
    // A million input tokens at $3/M is exactly $3.
    let usage = AIUsage::new(1_000_000, 0);
    assert_eq!(usage.cost(AIPricing::new(3.0, 15.0)), 3.0);
    let usage = AIUsage::new(0, 1_000_000);
    assert_eq!(usage.cost(AIPricing::new(3.0, 15.0)), 15.0);
  }

  #[test]
  fn cache_reads_bill_at_their_own_rate() {
    let pricing = AIPricing::new(3.0, 15.0).cache_read(0.3);
    let usage = AIUsage::new(0, 0).cache_read(2_000_000);
    assert!((usage.cost(pricing) - 0.6).abs() < 1e-9);
  }

  #[test]
  fn usage_adds_without_overflowing() {
    let huge = AIUsage {
      input: u64::MAX,
      output: u64::MAX,
      cache_read: u64::MAX,
    };
    assert_eq!((huge + huge).input, u64::MAX);
    assert_eq!(huge.total(), u64::MAX);
    // Summing a session's requests uses the same saturating add.
    let total: AIUsage = [AIUsage::new(1, 2), AIUsage::new(3, 4)].into_iter().sum();
    assert_eq!(total, AIUsage::new(4, 6));
  }

  #[test]
  fn tiny_amounts_stay_legible_and_bad_input_reads_as_zero() {
    assert_eq!(format_cost(0.0), "$0.00");
    assert_eq!(format_cost(-1.0), "$0.00");
    assert_eq!(format_cost(f64::NAN), "$0.00");
    assert_eq!(format_cost(0.00042), "$0.0004");
    assert_eq!(format_cost(0.125), "$0.125");
    assert_eq!(format_cost(12.3456), "$12.35");
  }
}
