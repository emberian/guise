//! `AISettings` — temperature and the output cap.
//!
//! Two knobs that every provider takes and every app ends up re-implementing.
//! Temperature is a slider because the useful range is narrow and the exact
//! value rarely matters; max tokens is a number field because it does. Both
//! clamp before they emit, so a host can wire the event straight into a
//! request without re-validating.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, EventEmitter, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::input::{NumberInput, NumberInputEvent, Slider, SliderEvent};
use crate::theme::{theme, Size};

/// The widest temperature any mainstream provider accepts.
const TEMPERATURE_MAX: f64 = 2.0;
/// A floor on the output cap; zero would make every request fail.
const MIN_OUTPUT: f64 = 1.0;

/// Emitted when a parameter changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AISettingsEvent {
    /// Clamped to `0.0..=2.0`.
    Temperature(f64),
    /// Clamped to at least 1, and to the configured ceiling.
    MaxTokens(u64),
}

/// Sampling controls for a request.
pub struct AISettings {
    temperature: Entity<Slider>,
    max_tokens: Entity<NumberInput>,
    /// The largest output the selected model allows.
    ceiling: u64,
    size: Size,
}

impl EventEmitter<AISettingsEvent> for AISettings {}

impl AISettings {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let temperature = cx.new(|cx| {
            Slider::new(cx)
                .min(0.0)
                .max(TEMPERATURE_MAX)
                .step(0.05)
                .value(1.0)
        });
        let max_tokens = cx.new(|cx| {
            NumberInput::new(cx)
                .min(MIN_OUTPUT)
                .max(4096.0)
                .step(256.0)
                .value(1024.0)
                .label("Max tokens")
        });

        cx.subscribe(&temperature, |_this, _slider, event: &SliderEvent, cx| {
            cx.emit(AISettingsEvent::Temperature(
                event.0.clamp(0.0, TEMPERATURE_MAX),
            ));
        })
        .detach();
        cx.subscribe(&max_tokens, |this, _input, event: &NumberInputEvent, cx| {
            cx.emit(AISettingsEvent::MaxTokens(this.clamp_output(event.0)));
        })
        .detach();

        AISettings {
            temperature,
            max_tokens,
            ceiling: 4096,
            size: Size::Sm,
        }
    }

    /// Start at this temperature.
    pub fn temperature(self, value: f64, cx: &mut Context<Self>) -> Self {
        let value = value.clamp(0.0, TEMPERATURE_MAX);
        self.temperature
            .update(cx, |slider, cx| slider.set_value(value, cx));
        self
    }

    /// Start at this output cap.
    pub fn max_tokens(self, value: u64, cx: &mut Context<Self>) -> Self {
        let value = self.clamp_output(value as f64);
        self.max_tokens
            .update(cx, |input, cx| input.set_value(value as f64, cx));
        self
    }

    /// The largest output the selected model allows. Lowering it below the
    /// current value pulls the value down with it, so the pair can never
    /// describe an impossible request.
    pub fn ceiling(&mut self, tokens: u64, cx: &mut Context<Self>) {
        self.ceiling = tokens.max(MIN_OUTPUT as u64);
        let ceiling = self.ceiling;
        self.max_tokens.update(cx, |input, cx| {
            let current = input.value_f64().unwrap_or(0.0);
            input.set_max(ceiling as f64, cx);
            if current > ceiling as f64 {
                input.set_value(ceiling as f64, cx);
            }
        });
        cx.notify();
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// The current temperature.
    pub fn temperature_value(&self, cx: &gpui::App) -> f64 {
        self.temperature
            .read(cx)
            .value_f64()
            .clamp(0.0, TEMPERATURE_MAX)
    }

    /// The current output cap.
    pub fn max_tokens_value(&self, cx: &gpui::App) -> u64 {
        self.clamp_output(self.max_tokens.read(cx).value_f64().unwrap_or(0.0))
    }

    /// Fold a raw number into a value a request can actually carry: finite,
    /// at least one token, no more than the model's ceiling.
    fn clamp_output(&self, raw: f64) -> u64 {
        if !raw.is_finite() {
            return MIN_OUTPUT as u64;
        }
        raw.clamp(MIN_OUTPUT, self.ceiling as f64).round() as u64
    }
}

impl Render for AISettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let font = t.font_size(self.size);
        let font_xs = t.font_size(Size::Xs);
        let dimmed = t.dimmed().hsla();
        let text_color = t.text().hsla();
        let temperature = self.temperature_value(cx);

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .w_full()
            .text_size(px(font))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_color(text_color)
                                    .child(SharedString::new_static("Temperature")),
                            )
                            .child(
                                div()
                                    .text_size(px(font_xs))
                                    .text_color(dimmed)
                                    .child(SharedString::from(format!("{temperature:.2}"))),
                            ),
                    )
                    .child(self.temperature.clone())
                    .child(div().text_size(px(font_xs)).text_color(dimmed).child(
                        SharedString::new_static(
                            "Lower is more predictable; higher is more varied.",
                        ),
                    )),
            )
            .child(self.max_tokens.clone())
            .probe("AISettings")
    }
}
