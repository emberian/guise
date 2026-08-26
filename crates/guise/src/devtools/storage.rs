//! The Storage panel: a sidebar of domains, a table of what is in the selected
//! one.
//!
//! Safari lists Local Storage, Session Storage, Cookies and the databases an
//! origin owns. A native app has the same shapes under different names —
//! preferences, an in-memory session, a cache, a SQLite file — so the host
//! names its own domains with [`super::storage_set`] and this renders them
//! under Safari's headings.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, SharedString, Window};

use super::shell::{
  cell, empty_state, glyph, header_cell, Ink, LABEL_SIZE, MONO_SIZE, NAV_WIDTH, ROW_HEIGHT,
};
use super::state::{format_bytes, DevToolsState, StorageDomain, StorageKind};
use super::DevTools;
use crate::icon::IconName;
use crate::style::{TextOverflowExt, MONO_FAMILY};

#[derive(Default)]
pub struct StoragePanel {
  /// The selected domain's id. Held by id rather than index so a host adding
  /// a domain does not move the selection out from under the user.
  selected: Option<SharedString>,
}

impl StoragePanel {
  fn icon(kind: StorageKind) -> IconName {
    match kind {
      StorageKind::Local => IconName::HardDrive,
      StorageKind::Session => IconName::Clock,
      StorageKind::Cookies => IconName::Cookie,
      StorageKind::Database => IconName::Database,
      StorageKind::Cache => IconName::Package,
    }
  }

  /// Group domains under their kind, preserving registration order within a
  /// group, and keeping the groups in the order Safari lists them.
  pub fn grouped(domains: &[StorageDomain]) -> Vec<(StorageKind, Vec<&StorageDomain>)> {
    let order = [
      StorageKind::Local,
      StorageKind::Session,
      StorageKind::Cookies,
      StorageKind::Database,
      StorageKind::Cache,
    ];
    order
      .into_iter()
      .filter_map(|kind| {
        let group: Vec<&StorageDomain> = domains
          .iter()
          .filter(|domain| domain.kind == kind)
          .collect();
        (!group.is_empty()).then_some((kind, group))
      })
      .collect()
  }

  pub fn render(&self, window: &mut Window, cx: &mut Context<DevTools>) -> AnyElement {
    let ink = Ink::read(cx);
    let domains: Vec<StorageDomain> = cx
      .try_global::<DevToolsState>()
      .map(|state| state.storage().to_vec())
      .unwrap_or_default();

    if domains.is_empty() {
      return empty_state(
        "No storage domains registered. Publish one with devtools::storage_set.",
        &ink,
      )
      .into_any_element();
    }

    let selected = self
      .selected
      .as_ref()
      .and_then(|id| domains.iter().find(|domain| &domain.id == id))
      .or_else(|| domains.first());

    let _ = window;

    div()
      .flex()
      .flex_1()
      .min_h(px(0.0))
      .w_full()
      .child(self.sidebar(&domains, selected.map(|d| &d.id), &ink, cx))
      .child(match selected {
        Some(domain) => self.table(domain, &ink),
        None => empty_state("Select a domain", &ink).into_any_element(),
      })
      .into_any_element()
  }

  fn sidebar(
    &self,
    domains: &[StorageDomain],
    selected: Option<&SharedString>,
    ink: &Ink,
    cx: &mut Context<DevTools>,
  ) -> AnyElement {
    let mut nav = div()
      .id("devtools-storage-nav")
      .flex()
      .flex_col()
      .flex_none()
      .w(px(NAV_WIDTH))
      .h_full()
      .overflow_scroll()
      .bg(ink.chrome)
      .border_r_1()
      .border_color(ink.border);

    let mut position = 0usize;
    for (kind, group) in Self::grouped(domains) {
      nav = nav.child(
        div()
          .flex()
          .flex_none()
          .items_center()
          .h(px(20.0))
          .w_full()
          .px(px(8.0))
          .text_size(px(LABEL_SIZE))
          .text_color(ink.dim)
          .child(SharedString::new_static(kind.label())),
      );

      for domain in group {
        let is_selected = selected == Some(&domain.id);
        let id = domain.id.clone();
        let hover_bg = ink.hover;
        let fg = if is_selected {
          ink.selected_text
        } else {
          ink.text
        };

        nav = nav.child(
          div()
            .id(("devtools-storage-domain", position))
            .flex()
            .items_center()
            .gap(px(5.0))
            .flex_none()
            .h(px(ROW_HEIGHT))
            .w_full()
            .pl(px(18.0))
            .pr(px(8.0))
            .text_size(px(LABEL_SIZE))
            .text_color(fg)
            .when(is_selected, |el| el.bg(ink.selected))
            .when(!is_selected, |el| el.hover(move |st| st.bg(hover_bg)))
            .child(glyph(Self::icon(domain.kind), 11.0, fg, cx))
            .child(div().flex_1().truncate_text().child(domain.name.clone()))
            .child(
              div()
                .flex_none()
                .text_color(if is_selected {
                  ink.selected_text
                } else {
                  ink.dim
                })
                .child(SharedString::from(domain.entries.len().to_string())),
            )
            .on_click(
              cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                this.storage.selected = Some(id.clone());
                cx.notify();
              }),
            ),
        );
        position += 1;
      }
    }

    nav.into_any_element()
  }

  fn table(&self, domain: &StorageDomain, ink: &Ink) -> AnyElement {
    let extra_width = 120.0;

    let mut header = div()
      .flex()
      .flex_none()
      .items_center()
      .h(px(20.0))
      .w_full()
      .bg(ink.chrome)
      .border_b_1()
      .border_color(ink.border)
      .child(header_cell("Key", Some(180.0), ink))
      .child(header_cell("Value", None, ink));
    for column in &domain.columns {
      header = header.child(header_cell(column.clone(), Some(extra_width), ink));
    }

    let mut rows = div()
      .id("devtools-storage-rows")
      .flex()
      .flex_col()
      .flex_1()
      .min_h(px(0.0))
      .w_full()
      .overflow_scroll()
      .bg(ink.content)
      .font_family(MONO_FAMILY)
      .text_size(px(MONO_SIZE));

    if domain.entries.is_empty() {
      rows = rows.child(
        div()
          .flex()
          .flex_1()
          .items_center()
          .justify_center()
          .w_full()
          .text_size(px(13.0))
          .text_color(ink.dim)
          .child(SharedString::new_static("This domain is empty")),
      );
    }

    for (position, entry) in domain.entries.iter().enumerate() {
      let mut row = div()
        .flex()
        .items_center()
        .flex_none()
        .h(px(ROW_HEIGHT))
        .w_full()
        .when(position % 2 == 1, |el| el.bg(ink.stripe))
        .child(cell(entry.key.clone(), Some(180.0), ink.attr))
        .child(cell(entry.value.clone(), None, ink.text));

      // Extra columns are matched by name, so a row missing one renders
      // blank rather than shifting the others along.
      for column in &domain.columns {
        let value = entry
          .extra
          .iter()
          .find(|(name, _)| name == column)
          .map(|(_, value)| value.clone())
          .unwrap_or_default();
        row = row.child(cell(value, Some(extra_width), ink.dim));
      }

      rows = rows.child(row);
    }

    div()
      .flex()
      .flex_col()
      .flex_1()
      .min_w(px(0.0))
      .h_full()
      .child(header)
      .child(rows)
      .child(
        div()
          .flex()
          .flex_none()
          .items_center()
          .gap(px(12.0))
          .h(px(20.0))
          .w_full()
          .px(px(8.0))
          .bg(ink.chrome)
          .border_t_1()
          .border_color(ink.border)
          .text_size(px(LABEL_SIZE))
          .text_color(ink.dim)
          .child(SharedString::from(format!(
            "{} entries",
            domain.entries.len()
          )))
          .child(SharedString::from(format_bytes(domain.size()))),
      )
      .into_any_element()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::devtools::state::StorageEntry;

  fn domain(id: &str, kind: StorageKind) -> StorageDomain {
    StorageDomain::new(id.to_string(), id.to_string()).kind(kind)
  }

  #[test]
  fn groups_follow_safaris_order_not_registration_order() {
    let domains = vec![
      domain("cache", StorageKind::Cache),
      domain("prefs", StorageKind::Local),
      domain("cookies", StorageKind::Cookies),
    ];

    let kinds: Vec<StorageKind> = StoragePanel::grouped(&domains)
      .into_iter()
      .map(|(kind, _)| kind)
      .collect();
    assert_eq!(
      kinds,
      vec![StorageKind::Local, StorageKind::Cookies, StorageKind::Cache]
    );
  }

  #[test]
  fn domains_keep_registration_order_within_a_group() {
    let domains = vec![
      domain("b", StorageKind::Local),
      domain("a", StorageKind::Local),
    ];
    let (_, group) = StoragePanel::grouped(&domains).remove(0);
    assert_eq!(group[0].id.as_ref(), "b");
    assert_eq!(group[1].id.as_ref(), "a");
  }

  #[test]
  fn an_empty_kind_produces_no_heading() {
    let domains = vec![domain("prefs", StorageKind::Local)];
    assert_eq!(StoragePanel::grouped(&domains).len(), 1);
  }

  #[test]
  fn size_counts_keys_and_values() {
    let domain = StorageDomain::new("prefs", "Preferences")
      .entry(StorageEntry::new("ab", "cde"))
      .entry(StorageEntry::new("f", "g"));
    assert_eq!(domain.size(), 7);
  }
}
