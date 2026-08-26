//! `Markdown` — read-only markdown, rendered.
//!
//! [`MarkdownEditor`](super::MarkdownEditor) exists to *edit* markdown, so it
//! carries a caret, a scroll model, and hand-rolled glyph layout. Displaying
//! markdown needs none of that, and asking for an editor to show a paragraph
//! is the wrong shape. This walks the same three pure passes the editor does —
//! [`block::classify`], then [`layout::plan`] with reveal off — and hands each
//! line to gpui's `StyledText`, which wraps it for us.
//!
//! It is what an assistant's reply is drawn with, and it is a plain
//! `RenderOnce` builder, so it can appear anywhere text can.
//!
//! ```ignore
//! div().child(Markdown::new("# Notes\n\n- **bold** and `code`"))
//! ```

use gpui::prelude::*;
use gpui::{
  div, px, App, Font, FontStyle, FontWeight, IntoElement, SharedString, StrikethroughStyle,
  StyledText, TextRun, UnderlineStyle, Window,
};

use super::block::{classify, DocState};
use super::layout::{metrics, plan, RowKind, RowPlan};
use crate::devtools::Probed;
use crate::style::MONO_FAMILY;
use crate::theme::{theme, ColorName, Size};

/// Read-only markdown. Create with [`Markdown::new`] and drop it anywhere.
#[derive(IntoElement)]
pub struct Markdown {
  source: SharedString,
  size: Size,
  /// Colors links with the theme's primary and underlines them.
  accent: Option<ColorName>,
  /// Cap on how much of the source is rendered, in lines. Streaming replies
  /// can get long, and the caller may want a preview.
  max_lines: Option<usize>,
}

impl Markdown {
  pub fn new(source: impl Into<SharedString>) -> Self {
    Markdown {
      source: source.into(),
      size: Size::Sm,
      accent: None,
      max_lines: None,
    }
  }

  /// Base text size; headings and code scale from it.
  pub fn size(mut self, size: Size) -> Self {
    self.size = size;
    self
  }

  /// Draw links in this palette color rather than the theme's primary.
  pub fn accent(mut self, accent: ColorName) -> Self {
    self.accent = Some(accent);
    self
  }

  /// Render at most `lines` source lines.
  pub fn max_lines(mut self, lines: usize) -> Self {
    self.max_lines = Some(lines);
    self
  }
}

impl RenderOnce for Markdown {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    // Start from the font actually in effect, so bold and italic are that
    // family's own faces. Building a `Font` from scratch with an empty
    // family name resolves to nothing and loses the weight with it.
    let prose = window.text_style().font();
    let base = t.font_size(self.size);
    let text_color = t.text().hsla();
    let dimmed = t.dimmed().hsla();
    let border = t.border().hsla();
    let accent = self
      .accent
      .map_or_else(|| t.primary(), |name| t.color(name, 6))
      .hsla();
    let code_bg = t.surface_hover().alpha(0.7);
    let mut highlight_bg = t.color(ColorName::Yellow, 3).hsla();
    highlight_bg.a = 0.45;

    // Cloned once instead of per run: `family` is a `SharedString` and
    // `..prose.clone()` would bump its refcount for every styled span on
    // every line, every frame.
    let prose_family = prose.family.clone();
    let mono_family: SharedString = MONO_FAMILY.into();
    let colors = RowColors {
      text: text_color,
      dimmed,
      border,
      code_bg,
    };

    let mut state = DocState::default();
    let mut column = div().flex().flex_col().w_full();
    let lines = self
      .source
      .lines()
      .take(self.max_lines.unwrap_or(usize::MAX));

    for line in lines {
      let block = classify(line, &mut state);
      // `reveal: false` is the whole difference from the editor: markers
      // stay hidden because there is no cursor that might want to edit
      // them. The fence's language is passed as `None` because nothing
      // here highlights code — carrying it would allocate a `String` per
      // line of every fenced block, every frame, to be thrown away.
      let row = plan(line, &block, None, false);
      let row_metrics = metrics(&row.kind);
      let size = base * row_metrics.scale;
      let weight = if matches!(row.kind, RowKind::Heading(_)) {
        FontWeight::BOLD
      } else {
        prose.weight
      };

      let runs: Vec<TextRun> = row
        .runs
        .iter()
        .filter(|run| run.len > 0)
        .map(|run| {
          let s = run.style;
          let mono = s.code || matches!(row.kind, RowKind::Code { .. });
          TextRun {
            len: run.len,
            font: Font {
              family: if mono {
                mono_family.clone()
              } else {
                prose_family.clone()
              },
              weight: if s.bold { FontWeight::BOLD } else { weight },
              style: if s.italic {
                FontStyle::Italic
              } else {
                prose.style
              },
              ..prose.clone()
            },
            color: if run.marker || run.dim {
              dimmed
            } else if s.link {
              accent
            } else {
              text_color
            },
            background_color: if s.highlight {
              Some(highlight_bg)
            } else if s.code && !mono_block(&row.kind) {
              Some(code_bg)
            } else {
              None
            },
            underline: (s.link && !run.marker).then(|| UnderlineStyle {
              thickness: px(1.0),
              color: Some(accent),
              wavy: false,
            }),
            strikethrough: s.strike.then(|| StrikethroughStyle {
              thickness: px(1.0),
              color: Some(dimmed),
            }),
          }
        })
        .collect();

      column = column.child(row_element(row, runs, size, base, &colors));
    }
    column.probe("Markdown")
  }
}

/// The colors a row draws with, resolved once per render. Passed as one value
/// because four bare `Hsla` parameters in a row are silently transposable.
struct RowColors {
  text: gpui::Hsla,
  dimmed: gpui::Hsla,
  border: gpui::Hsla,
  code_bg: gpui::Hsla,
}

/// The chrome around one line: bullets, checkboxes, quote bars, code
/// backgrounds, and the rule.
fn row_element(
  row: RowPlan,
  runs: Vec<TextRun>,
  size: f32,
  base: f32,
  colors: &RowColors,
) -> gpui::Div {
  let m = metrics(&row.kind);
  // `visible` is moved out of the plan rather than cloned: the plan is this
  // frame's and nothing else reads it.
  let text = StyledText::new(SharedString::from(row.visible)).with_runs(runs);

  let mut line = div()
    .flex()
    .flex_row()
    .items_start()
    .w_full()
    .text_size(px(size))
    .pt(px(base * m.pad_top))
    .pb(px(base * m.pad_bottom));

  match &row.kind {
    // A blank line is vertical space, not an empty text box that would
    // collapse to nothing.
    RowKind::Blank => return div().h(px(base * 0.6)),
    RowKind::Rule => {
      return div()
        .py(px(base * 0.5))
        .child(div().w_full().h(px(1.0)).bg(colors.border))
    }
    // The fence markers themselves are syntax, so they leave no row.
    RowKind::Fence { .. } | RowKind::FrontMatter => return div(),
    RowKind::Code { .. } => {
      return div()
        .w_full()
        .px(px(base * 0.6))
        .bg(colors.code_bg)
        .text_size(px(size))
        .child(text);
    }
    RowKind::Quote { depth } => {
      for _ in 0..(*depth).max(1) {
        line = line.child(
          div()
            .flex_none()
            .w(px(2.0))
            .h(px(base * 1.4))
            .mr(px(base * 0.6))
            .bg(colors.border),
        );
      }
    }
    RowKind::Bullet { cols } => {
      line = line
        .pl(px(*cols as f32 * base * 0.5))
        .child(marker(base, colors.dimmed, "\u{2022}"));
    }
    RowKind::Ordered { cols, number } => {
      line = line.pl(px(*cols as f32 * base * 0.5)).child(marker(
        base,
        colors.dimmed,
        format!("{number}."),
      ));
    }
    RowKind::Task { cols, checked } => {
      let glyph = if *checked { "\u{2611}" } else { "\u{2610}" };
      line = line.pl(px(*cols as f32 * base * 0.5)).child(marker(
        base,
        if *checked { colors.dimmed } else { colors.text },
        glyph,
      ));
    }
    RowKind::Heading(_) | RowKind::Paragraph | RowKind::Table => {}
  }

  line.child(div().flex_1().min_w(px(0.0)).child(text))
}

/// The bullet, number, or checkbox in a list row's gutter.
fn marker(base: f32, color: gpui::Hsla, glyph: impl Into<SharedString>) -> gpui::Div {
  div()
    .flex_none()
    .min_w(px(base * 1.1))
    .mr(px(base * 0.4))
    .text_color(color)
    .child(glyph.into())
}

/// Whether the whole row is already monospaced, so an inline-code run should
/// not paint its own background on top.
fn mono_block(kind: &RowKind) -> bool {
  matches!(kind, RowKind::Code { .. } | RowKind::Fence { .. })
}
