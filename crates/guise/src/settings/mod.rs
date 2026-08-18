//! Settings UI: the shell, the groups, and the rows.
//!
//! Three apps built the same settings screen three times before this module
//! existed, and the copies had already drifted — one marked an overridden key
//! with a reset arrow, another with a dot. What they shared was the *chrome*:
//! a page list beside a scrolling pane, groups with a heading and a rule, and
//! rows with the name on the left and the control on the right.
//!
//! So that is what lives here, and nothing else. There is no schema type, no
//! `Setting` trait, no value marshalling. Every app types its settings against
//! its own config struct — `fn(&Options) -> bool` and friends — and a component
//! generic enough to hold those would push the cost back onto the caller in
//! type parameters or stringly-typed values. The schema is also the part worth
//! owning: it is the product surface, and it is a couple of hundred lines.
//!
//! ```ignore
//! let settings = cx.new(|cx| {
//!     SettingsView::new(cx)
//!         .page("appearance", "Appearance")
//!         .page("editor", "Editor")
//!         .searchable(true)
//!         .content(|page, query, _window, cx| match page {
//!             "appearance" => appearance_page(query, cx),
//!             _ => editor_page(query, cx),
//!         })
//! });
//! ```
//!
//! A page is then built from the pieces:
//!
//! ```ignore
//! SettingsSection::new("Theme")
//!     .description("How the app looks.")
//!     .child(
//!         SettingsRow::new("dark", "Dark mode")
//!             .description("Follow the system, or pin one.")
//!             .modified(options.pinned("theme"))
//!             .on_reset(cx.listener(|this, _, _, cx| this.reset("theme", cx)))
//!             .control(Switch::new("theme").checked(dark)),
//!     )
//! ```

mod row;
mod section;
mod view;

pub use row::SettingsRow;
pub use section::SettingsSection;
pub use view::{SettingsPage, SettingsView, SettingsViewEvent};
