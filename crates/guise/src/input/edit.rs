//! Pure single-line text-editing model: a string plus a char-index cursor,
//! with the operations a text field needs. No UI — fully unit-testable; the
//! `TextInput` entity drives it from key events and renders from `split`.

/// How many undo steps a field remembers.
const UNDO_DEPTH: usize = 128;

/// …and how much text those steps may hold between them, in chars.
///
/// A step is a whole copy of the buffer, which is nothing for a one-line field
/// and a great deal for a `TextArea` holding a document: 128 steps of a 200 KB
/// buffer is 100 MB of `Vec<char>`. Bounding the depth alone leaves that hole,
/// so history is bounded by what it *retains* as well, and the oldest steps
/// are dropped first. A quarter-million chars is far more history than anyone
/// undoes through, and it costs a megabyte at worst.
const UNDO_CHARS: usize = 256 * 1024;

/// What the last mutation was, so a run of the same kind coalesces into one
/// undo step instead of one step per keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
  Insert,
  Delete,
}

/// A restorable point in the edit history.
#[derive(Debug, Clone)]
struct Snapshot {
  chars: Vec<char>,
  cursor: usize,
  anchor: Option<usize>,
}

/// An editable line of text with a cursor and an optional selection.
#[derive(Debug, Clone, Default)]
pub struct TextEdit {
  chars: Vec<char>,
  /// Cursor position as a char index in `0..=chars.len()`.
  cursor: usize,
  /// Selection anchor; a selection spans `anchor..cursor` in either order.
  /// `None` means no selection.
  anchor: Option<usize>,
  undos: Vec<Snapshot>,
  redos: Vec<Snapshot>,
  /// The kind of the mutation that produced the newest undo entry, so the
  /// next one of the same kind can fold into it.
  run: Option<EditKind>,
}

impl TextEdit {
  /// Start editing `text` with the cursor at the end.
  pub fn new(text: &str) -> Self {
    let chars: Vec<char> = text.chars().collect();
    let cursor = chars.len();
    Self {
      chars,
      cursor,
      anchor: None,
      undos: Vec::new(),
      redos: Vec::new(),
      run: None,
    }
  }

  pub fn text(&self) -> String {
    self.chars.iter().collect()
  }

  /// The buffer itself. Callers that only need to measure or slice the text
  /// use this instead of [`text`](Self::text), which allocates a fresh
  /// `String` every call — and the platform's input handler asks several
  /// times per keystroke.
  pub fn chars(&self) -> &[char] {
    &self.chars
  }

  pub fn is_empty(&self) -> bool {
    self.chars.is_empty()
  }

  /// Length in chars, which is also the last valid cursor position.
  pub fn len(&self) -> usize {
    self.chars.len()
  }

  /// The cursor as a char index.
  pub fn cursor(&self) -> usize {
    self.cursor
  }

  /// Move the cursor, dropping any selection. Out-of-range values clamp.
  pub fn set_cursor(&mut self, index: usize) {
    self.cursor = index.min(self.chars.len());
    self.anchor = None;
    self.run = None;
  }

  /// Select `start..end` and leave the cursor at `end`, so a following
  /// Shift+Arrow extends from the edge the user last moved.
  pub fn set_selection(&mut self, start: usize, end: usize) {
    let n = self.chars.len();
    self.anchor = Some(start.min(n));
    self.cursor = end.min(n);
    self.run = None;
  }

  /// Move the selection's free edge to `index`, opening a selection anchored
  /// at the cursor if there isn't one. This is Shift+click and mouse drag.
  pub fn extend_to(&mut self, index: usize) {
    if self.anchor.is_none() {
      self.anchor = Some(self.cursor);
    }
    self.cursor = index.min(self.chars.len());
    self.run = None;
  }

  /// Replace everything, as a programmatic set: the history is dropped
  /// because the old text is no longer something the user can undo back to.
  /// That also releases whatever the history was holding.
  pub fn set_text(&mut self, text: &str) {
    *self = TextEdit::new(text);
  }

  /// Byte offset into [`text`](Self::text) for a char index. Shaped-line
  /// geometry is addressed in bytes, the edit model in chars.
  pub fn byte_of(&self, index: usize) -> usize {
    self.chars[..index.min(self.chars.len())]
      .iter()
      .map(|c| c.len_utf8())
      .sum()
  }

  /// Char index for a byte offset, rounding up to the next char boundary.
  pub fn char_of(&self, byte: usize) -> usize {
    let mut at = 0;
    for (index, c) in self.chars.iter().enumerate() {
      if at >= byte {
        return index;
      }
      at += c.len_utf8();
    }
    self.chars.len()
  }

  /// The word surrounding `index` as `(start, end)` char indices — what a
  /// double-click selects. A click in whitespace takes the whitespace run.
  pub fn word_at(&self, index: usize) -> (usize, usize) {
    let n = self.chars.len();
    if n == 0 {
      return (0, 0);
    }
    // A click at the very end, or just past a word, belongs to the char
    // before it — the same way a browser resolves the boundary case.
    let probe = index.min(n - 1);
    let wordish = is_word(self.chars[probe]);
    let mut start = probe;
    while start > 0 && is_word(self.chars[start - 1]) == wordish {
      start -= 1;
    }
    let mut end = probe;
    while end < n && is_word(self.chars[end]) == wordish {
      end += 1;
    }
    (start, end)
  }

  /// Undo the last edit. Returns whether there was one.
  pub fn undo(&mut self) -> bool {
    let Some(previous) = self.undos.pop() else {
      return false;
    };
    self.redos.push(self.snapshot());
    self.restore(previous);
    true
  }

  /// Redo the last undone edit. Returns whether there was one.
  pub fn redo(&mut self) -> bool {
    let Some(next) = self.redos.pop() else {
      return false;
    };
    self.undos.push(self.snapshot());
    self.restore(next);
    true
  }

  fn snapshot(&self) -> Snapshot {
    Snapshot {
      chars: self.chars.clone(),
      cursor: self.cursor,
      anchor: self.anchor,
    }
  }

  fn restore(&mut self, snapshot: Snapshot) {
    self.chars = snapshot.chars;
    self.cursor = snapshot.cursor.min(self.chars.len());
    self.anchor = snapshot.anchor.map(|a| a.min(self.chars.len()));
    self.run = None;
  }

  /// Remember the pre-edit state, unless this continues a run of the same
  /// kind — typing a word is one undo step, not one per letter.
  fn record(&mut self, kind: EditKind) {
    self.redos.clear();
    if self.run == Some(kind) {
      return;
    }
    self.undos.push(self.snapshot());
    // Oldest first: the step you are least likely to reach for is the one
    // furthest back.
    while self.undos.len() > UNDO_DEPTH
      || (self.history_chars() > UNDO_CHARS && self.undos.len() > 1)
    {
      self.undos.remove(0);
    }
    self.run = Some(kind);
  }

  /// Chars held by the undo history. Summed on demand rather than tracked
  /// incrementally: the stacks are at most [`UNDO_DEPTH`] deep and this is
  /// only asked while trimming, which is nowhere near a hot path.
  pub(crate) fn history_chars(&self) -> usize {
    self
      .undos
      .iter()
      .chain(&self.redos)
      .map(|snapshot| snapshot.chars.len())
      .sum()
  }

  /// End the current coalescing run, so the next edit starts a fresh undo
  /// step. Callers use this at word boundaries and on paste.
  pub fn break_undo(&mut self) {
    self.run = None;
  }

  /// The selected span `(start, end)` as char indices, or `None` if the
  /// selection is empty/collapsed.
  pub fn selection(&self) -> Option<(usize, usize)> {
    let a = self.anchor?;
    (a != self.cursor).then(|| (a.min(self.cursor), a.max(self.cursor)))
  }

  pub fn has_selection(&self) -> bool {
    self.selection().is_some()
  }

  /// The selected text, or `None` when nothing is selected.
  pub fn selected_text(&self) -> Option<String> {
    let (s, e) = self.selection()?;
    Some(self.chars[s..e].iter().collect())
  }

  /// Select the whole line.
  pub fn select_all(&mut self) {
    if self.chars.is_empty() {
      self.anchor = None;
      return;
    }
    self.anchor = Some(0);
    self.cursor = self.chars.len();
  }

  /// Drop any selection, keeping the cursor put.
  pub fn clear_selection(&mut self) {
    self.anchor = None;
  }

  pub fn collapse_selection_start(&mut self) -> bool {
    let Some((start, _)) = self.selection() else {
      return false;
    };
    self.cursor = start;
    self.anchor = None;
    true
  }

  pub fn collapse_selection_end(&mut self) -> bool {
    let Some((_, end)) = self.selection() else {
      return false;
    };
    self.cursor = end;
    self.anchor = None;
    true
  }

  /// Delete the selected text (if any), leaving the cursor at its start.
  /// Returns whether anything was removed.
  pub fn delete_selection(&mut self) -> bool {
    if self.selection().is_none() {
      return false;
    }
    self.record(EditKind::Delete);
    self.take_selection()
  }

  /// [`delete_selection`](Self::delete_selection) without touching the undo
  /// history, for the mutators that have already recorded their own step.
  fn take_selection(&mut self) -> bool {
    let Some((s, e)) = self.selection() else {
      return false;
    };
    self.chars.drain(s..e);
    self.cursor = s;
    self.anchor = None;
    true
  }

  /// Prepare for a cursor move: with `extend` (Shift held) anchor a selection
  /// at the current cursor if one isn't already open; otherwise drop it.
  pub fn pre_move(&mut self, extend: bool) {
    if extend {
      if self.anchor.is_none() {
        self.anchor = Some(self.cursor);
      }
    } else {
      self.anchor = None;
    }
  }

  /// The text split around the selection: `(before, selected, after)`, or
  /// `None` when nothing is selected.
  pub fn split_selection(&self) -> Option<(String, String, String)> {
    let (s, e) = self.selection()?;
    Some((
      self.chars[..s].iter().collect(),
      self.chars[s..e].iter().collect(),
      self.chars[e..].iter().collect(),
    ))
  }

  /// Insert `s` at the cursor, replacing any selection, advancing past it.
  pub fn insert(&mut self, s: &str) {
    if s.is_empty() && !self.has_selection() {
      return;
    }
    self.record(EditKind::Insert);
    self.take_selection();
    // `splice` shifts the tail once. Inserting char by char shifts it per
    // character, which turns a large paste near the start of a long buffer
    // into quadratic work on the UI thread.
    let at = self.cursor;
    self.chars.splice(at..at, s.chars());
    self.cursor += s.chars().count();
    // Typing a word is one undo step; the space after it ends that step so
    // undo walks back word by word rather than wiping the whole line.
    if s.chars().any(|c| c.is_whitespace()) {
      self.run = None;
    }
  }

  /// Replace the char range `range` with `s`, leaving the cursor after it.
  /// This is the entry point the platform's text-input handler drives, so it
  /// takes an explicit range rather than using the selection.
  pub fn replace_range(&mut self, range: std::ops::Range<usize>, s: &str) {
    let n = self.chars.len();
    let start = range.start.min(n);
    let end = range.end.clamp(start, n);
    self.record(EditKind::Insert);
    self.chars.splice(start..end, s.chars());
    self.cursor = start + s.chars().count();
    self.anchor = None;
    if s.chars().any(|c| c.is_whitespace()) {
      self.run = None;
    }
  }

  /// Delete the selection, or the char before the cursor. Returns whether
  /// anything changed.
  pub fn backspace(&mut self) -> bool {
    if self.has_selection() {
      return self.delete_selection();
    }
    if self.cursor == 0 {
      return false;
    }
    self.record(EditKind::Delete);
    self.cursor -= 1;
    self.chars.remove(self.cursor);
    true
  }

  /// Delete the selection, or the char at the cursor. Returns whether anything
  /// changed.
  pub fn delete(&mut self) -> bool {
    if self.has_selection() {
      return self.delete_selection();
    }
    if self.cursor >= self.chars.len() {
      return false;
    }
    self.record(EditKind::Delete);
    self.chars.remove(self.cursor);
    true
  }

  pub fn left(&mut self) {
    self.cursor = self.cursor.saturating_sub(1);
  }

  pub fn right(&mut self) {
    if self.cursor < self.chars.len() {
      self.cursor += 1;
    }
  }

  pub fn home(&mut self) {
    self.cursor = 0;
  }

  pub fn end(&mut self) {
    self.cursor = self.chars.len();
  }

  pub fn line_home(&mut self) {
    while self.cursor > 0 && self.chars[self.cursor - 1] != '\n' {
      self.cursor -= 1;
    }
  }

  pub fn line_end(&mut self) {
    while self.cursor < self.chars.len() && self.chars[self.cursor] != '\n' {
      self.cursor += 1;
    }
  }

  /// Move left to the start of the previous word (Option+Left on macOS).
  pub fn word_left(&mut self) {
    while self.cursor > 0 && !is_word(self.chars[self.cursor - 1]) {
      self.cursor -= 1;
    }
    while self.cursor > 0 && is_word(self.chars[self.cursor - 1]) {
      self.cursor -= 1;
    }
  }

  /// Move right past the end of the next word (Option+Right on macOS).
  pub fn word_right(&mut self) {
    let n = self.chars.len();
    while self.cursor < n && !is_word(self.chars[self.cursor]) {
      self.cursor += 1;
    }
    while self.cursor < n && is_word(self.chars[self.cursor]) {
      self.cursor += 1;
    }
  }

  /// Delete the word before the cursor (Option+Backspace). Returns whether
  /// anything changed.
  pub fn delete_word_back(&mut self) -> bool {
    if self.has_selection() {
      return self.delete_selection();
    }
    let end = self.cursor;
    self.word_left();
    if self.cursor < end {
      self.record(EditKind::Delete);
      self.chars.drain(self.cursor..end);
      self.run = None;
      true
    } else {
      false
    }
  }

  /// Delete the word after the cursor (Option+Delete). Returns whether
  /// anything changed.
  pub fn delete_word_forward(&mut self) -> bool {
    if self.has_selection() {
      return self.delete_selection();
    }
    let start = self.cursor;
    let n = self.chars.len();
    let mut end = self.cursor;
    while end < n && !is_word(self.chars[end]) {
      end += 1;
    }
    while end < n && is_word(self.chars[end]) {
      end += 1;
    }
    if end > start {
      self.record(EditKind::Delete);
      self.chars.drain(start..end);
      self.run = None;
      true
    } else {
      false
    }
  }

  /// Delete from the cursor to the line start (Cmd+Backspace). Returns
  /// whether anything changed.
  pub fn delete_to_start(&mut self) -> bool {
    if self.has_selection() {
      return self.delete_selection();
    }
    if self.cursor == 0 {
      return false;
    }
    self.record(EditKind::Delete);
    self.chars.drain(0..self.cursor);
    self.cursor = 0;
    self.run = None;
    true
  }

  /// Delete from the cursor to the line end (Cmd+Delete / Ctrl+K). Returns
  /// whether anything changed.
  pub fn delete_to_end(&mut self) -> bool {
    if self.has_selection() {
      return self.delete_selection();
    }
    if self.cursor >= self.chars.len() {
      return false;
    }
    self.record(EditKind::Delete);
    self.chars.truncate(self.cursor);
    self.run = None;
    true
  }

  /// The text before and after the cursor, for rendering a caret between.
  pub fn split(&self) -> (String, String) {
    (
      self.chars[..self.cursor].iter().collect(),
      self.chars[self.cursor..].iter().collect(),
    )
  }

  /// Move the cursor up one line, keeping the column where possible. Multiline
  /// only (single-line text has nowhere to go).
  pub fn up(&mut self) {
    self.vmove(-1);
  }

  /// Move the cursor down one line, keeping the column where possible.
  pub fn down(&mut self) {
    self.vmove(1);
  }

  /// (line, column) of the cursor, counting `\n`-separated lines.
  fn line_col(&self) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for &c in &self.chars[..self.cursor] {
      if c == '\n' {
        line += 1;
        col = 0;
      } else {
        col += 1;
      }
    }
    (line, col)
  }

  /// (start char index, length excluding newline) for each line.
  fn line_bounds(&self) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut len = 0;
    for (i, &c) in self.chars.iter().enumerate() {
      if c == '\n' {
        out.push((start, len));
        start = i + 1;
        len = 0;
      } else {
        len += 1;
      }
    }
    out.push((start, len));
    out
  }

  fn vmove(&mut self, dir: isize) {
    let (line, col) = self.line_col();
    let bounds = self.line_bounds();
    let target = line as isize + dir;
    if target < 0 || target as usize >= bounds.len() {
      return;
    }
    let (start, len) = bounds[target as usize];
    self.cursor = start + col.min(len);
  }
}

/// Word characters for word-wise navigation/deletion.
fn is_word(c: char) -> bool {
  c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn new_places_cursor_at_end() {
    let e = TextEdit::new("abc");
    assert_eq!(e.split(), ("abc".into(), "".into()));
  }

  #[test]
  fn insert_at_cursor() {
    let mut e = TextEdit::new("ac");
    e.left();
    e.insert("b");
    assert_eq!(e.text(), "abc");
    assert_eq!(e.split(), ("ab".into(), "c".into()));
  }

  #[test]
  fn backspace_and_delete() {
    let mut e = TextEdit::new("abc");
    assert!(e.backspace());
    assert_eq!(e.text(), "ab");
    e.home();
    assert!(e.delete());
    assert_eq!(e.text(), "b");
    e.home();
    assert!(!e.backspace());
    e.end();
    assert!(!e.delete());
  }

  #[test]
  fn handles_unicode() {
    let mut e = TextEdit::new("café");
    assert!(e.backspace());
    assert_eq!(e.text(), "caf");
    e.insert("é");
    assert_eq!(e.text(), "café");
  }

  #[test]
  fn vertical_movement_keeps_column() {
    // Two lines: "hello" / "hi". Cursor starts at end ("hi").
    let mut e = TextEdit::new("hello\nhi");
    // Column 2 on line 1.
    e.up();
    // Same column (2) on line 0 → between "he" and "llo".
    assert_eq!(e.split().0, "he");
    e.down();
    // Back to line 1; column clamped to its length (2) → end.
    assert_eq!(e.split(), ("hello\nhi".into(), "".into()));
  }

  #[test]
  fn vertical_movement_stops_at_edges() {
    let mut e = TextEdit::new("a\nb");
    e.home(); // line 1 has only the final char; home goes to absolute start
    e.up(); // already on first line, no-op
    assert_eq!(e.split().0, "");
  }

  #[test]
  fn word_navigation() {
    let mut e = TextEdit::new("foo bar baz");
    e.word_left();
    assert_eq!(e.split(), ("foo bar ".into(), "baz".into()));
    e.word_left();
    assert_eq!(e.split(), ("foo ".into(), "bar baz".into()));
    e.word_right();
    assert_eq!(e.split(), ("foo bar".into(), " baz".into()));
  }

  #[test]
  fn delete_word_back_and_forward() {
    let mut e = TextEdit::new("foo bar baz");
    assert!(e.delete_word_back());
    assert_eq!(e.text(), "foo bar ");
    e.home();
    assert!(e.delete_word_forward());
    assert_eq!(e.text(), " bar ");
    // Nothing before the cursor at home: no-op.
    e.home();
    assert!(!e.delete_word_back());
  }

  #[test]
  fn select_all_then_type_replaces() {
    let mut e = TextEdit::new("hello");
    e.select_all();
    assert_eq!(e.selected_text().as_deref(), Some("hello"));
    e.insert("x");
    assert_eq!(e.text(), "x");
    assert!(!e.has_selection());
  }

  #[test]
  fn shift_arrow_extends_selection() {
    let mut e = TextEdit::new("abcd");
    e.pre_move(true);
    e.left(); // select "d"
    e.pre_move(true);
    e.left(); // select "cd"
    assert_eq!(e.selected_text().as_deref(), Some("cd"));
    let (before, sel, after) = e.split_selection().unwrap();
    assert_eq!(
      (before.as_str(), sel.as_str(), after.as_str()),
      ("ab", "cd", "")
    );
  }

  #[test]
  fn plain_move_clears_selection() {
    let mut e = TextEdit::new("abcd");
    e.select_all();
    e.pre_move(false);
    e.left();
    assert!(!e.has_selection());
  }

  #[test]
  fn backspace_deletes_selection() {
    let mut e = TextEdit::new("abcd");
    e.select_all();
    assert!(e.backspace());
    assert_eq!(e.text(), "");
  }

  #[test]
  fn modified_deletes_replace_the_selection() {
    for delete in [
      TextEdit::delete_word_back as fn(&mut TextEdit) -> bool,
      TextEdit::delete_word_forward,
      TextEdit::delete_to_start,
      TextEdit::delete_to_end,
    ] {
      let mut edit = TextEdit::new("abcd");
      edit.select_all();
      assert!(delete(&mut edit));
      assert_eq!(edit.text(), "");
    }
  }

  #[test]
  fn selection_collapses_to_the_requested_edge() {
    let mut edit = TextEdit::new("abcd");
    edit.select_all();
    assert!(edit.collapse_selection_start());
    assert_eq!(edit.split().0, "");
    edit.select_all();
    assert!(edit.collapse_selection_end());
    assert_eq!(edit.split().0, "abcd");
  }

  #[test]
  fn delete_to_line_edges() {
    let mut e = TextEdit::new("hello world");
    e.home();
    e.right();
    e.right();
    assert!(e.delete_to_start());
    assert_eq!(e.text(), "llo world");
    assert!(e.delete_to_end());
    assert_eq!(e.text(), "");
    assert!(!e.delete_to_end());
  }

  #[test]
  fn line_edges_stay_on_the_current_line() {
    let mut e = TextEdit::new("one\ntwo\nthree");
    e.up();
    e.line_home();
    assert_eq!(e.split().0, "one\n");
    e.line_end();
    assert_eq!(e.split().0, "one\ntwo");
  }

  /// An undo step is a whole copy of the buffer, so a big document must not
  /// be able to turn a bounded step count into unbounded memory.
  #[test]
  fn undo_history_is_bounded_by_the_text_it_retains() {
    let big = "x".repeat(64 * 1024);
    let mut edit = TextEdit::new(&big);
    // Each edit is its own step (the deletes break the coalescing run).
    for i in 0..64 {
      edit.insert(&format!("{i} "));
      edit.backspace();
    }
    assert!(
      edit.history_chars() <= UNDO_CHARS + big.chars().count(),
      "history held {} chars",
      edit.history_chars()
    );
    // The recent past still works even though the distant past was dropped.
    assert!(edit.undo());
    assert!(edit.undo());
  }

  #[test]
  fn undo_accounting_survives_a_round_trip() {
    let mut edit = TextEdit::new("");
    edit.insert("alpha ");
    edit.insert("beta");
    let before = edit.history_chars();
    assert!(edit.undo());
    assert!(edit.redo());
    assert_eq!(edit.history_chars(), before);
    assert_eq!(edit.text(), "alpha beta");
    // A fresh edit discards the redo stack and its accounting with it.
    edit.insert("!");
    assert!(edit.history_chars() > 0);
  }

  #[test]
  fn set_text_releases_the_history() {
    let mut edit = TextEdit::new(&"y".repeat(4096));
    edit.insert("a");
    edit.backspace();
    assert!(edit.history_chars() > 0);
    edit.set_text("small");
    assert_eq!(edit.history_chars(), 0);
    assert!(!edit.undo());
  }

  #[test]
  fn chars_matches_text_without_allocating() {
    let edit = TextEdit::new("caf\u{e9} \u{1f600}");
    assert_eq!(edit.chars().iter().collect::<String>(), edit.text());
    assert_eq!(edit.chars().len(), edit.len());
  }
}
