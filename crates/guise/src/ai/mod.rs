//! Components for putting a model in front of a person.
//!
//! Every one of these is UI. None of them opens a socket, holds a key, or
//! knows what a provider is — guise is gpui and std, and a component library
//! is the wrong place to keep someone's credentials. The host owns the
//! request; these own what the user sees while it happens.
//!
//! That split is also what makes them portable: the same [`AIChatView`] drives
//! a local model, a hosted API, or a replayed transcript, because all it ever
//! receives is text.
//!
//! # The short version
//!
//! ```ignore
//! use guise::ai::*;
//!
//! let chat = cx.new(|cx| AIChatView::new(cx).max_width(760.0));
//! let composer = cx.new(|cx| AIComposer::new(cx).hint("Shift+Enter for a new line"));
//!
//! cx.subscribe(&composer, move |this, _composer, event: &AIComposerEvent, cx| {
//!     if let AIComposerEvent::Submit(text) = event {
//!         this.chat.update(cx, |chat, cx| {
//!             chat.push(AITurn::user(text.clone()), cx);
//!             chat.begin_reply(cx);
//!         });
//!         this.send(text.clone(), cx); // your transport
//!     }
//! })
//! .detach();
//! ```
//!
//! As tokens arrive, `chat.push_delta(&token, cx)`; when the reply ends,
//! `chat.end_reply(cx)`, or `chat.fail_reply(error, cx)` if it didn't.
//!
//! # What's here
//!
//! - [`AIChatView`] — the transcript, with stick-to-bottom scrolling and
//!   per-turn state. [`AITurn`] is what goes in it.
//! - [`AIMessage`] — one turn, if you'd rather lay the list out yourself.
//! - [`AIComposer`] — the prompt box, with send/stop.
//! - [`AIStreamingText`] and [`AIThinking`] — a reply arriving, and the wait
//!   before it starts.
//! - [`AIReasoning`] — extended thinking, folded away.
//! - [`AIToolCall`] — what the model did and whether it worked.
//! - [`AICitation`] and [`AISources`] — where an answer came from.
//! - [`AIModelPicker`], [`AITokenMeter`], [`AICost`], [`AISettings`] — the
//!   controls and meters around a request.

mod chatview;
mod citation;
mod composer;
mod cost;
mod message;
mod modelpicker;
mod reasoning;
mod settings;
mod sources;
mod streamingtext;
mod thinking;
mod tokenmeter;
mod toolcall;

pub use chatview::{AIChatView, AIChatViewEvent, AITurn, AITurnTool};
pub use citation::{AICitation, AISource};
pub use composer::{AIComposer, AIComposerEvent};
pub use cost::{format_cost, AICost, AIPricing, AIUsage};
pub use message::{AIMessage, AIRole};
pub use modelpicker::{AIModel, AIModelPicker, AIModelPickerEvent};
pub use reasoning::AIReasoning;
pub use settings::{AISettings, AISettingsEvent};
pub use sources::AISources;
pub use streamingtext::AIStreamingText;
pub use thinking::AIThinking;
pub use tokenmeter::AITokenMeter;
pub use toolcall::{AIToolCall, AIToolStatus};
