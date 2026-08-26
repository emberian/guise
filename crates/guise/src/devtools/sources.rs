//! The Sources panel: the files the app reported, and their contents.
//!
//! Safari lists the resources a page loaded and shows them with line numbers.
//! A compiled binary has no resources to enumerate, but it does know where its
//! elements were constructed — every probe carries a `#[track_caller]`
//! location — so the file list is built from the tree, and the contents are
//! read off disk.
//!
//! Reading off disk is right for the case that matters: a debug build run from
//! its own checkout, which is where an inspector is used. When the file is not
//! there, the panel says so and still shows the location; it never guesses.

use std::collections::BTreeMap;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, SharedString, Window};

use super::probe::ProbeTree;
use super::shell::{empty_state, glyph, Ink, LABEL_SIZE, MONO_SIZE, NAV_WIDTH, ROW_HEIGHT};
use super::state::SourceRef;
use super::DevTools;
use crate::icon::IconName;
use crate::style::{TextOverflowExt, MONO_FAMILY};

/// Files past this size are listed but not opened. A source file is never this
/// big, so hitting it means the location pointed at something else.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Turn a `#[track_caller]` path into something openable.
///
/// `file!()` is relative to the *workspace* root, and a running binary's
/// working directory is rarely that — `cargo test` sits in the package, a
/// launched app sits wherever the launcher put it. So an absolute path is used
/// as-is, and a relative one is tried against the working directory and each
/// of its ancestors, which finds the checkout from anywhere inside it.
pub fn resolve(path: &str) -> Option<std::path::PathBuf> {
    let candidate = std::path::Path::new(path);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }

    let cwd = std::env::current_dir().ok()?;
    for root in cwd.ancestors() {
        let joined = root.join(candidate);
        if joined.is_file() {
            return Some(joined);
        }
    }
    None
}

/// What happened when the panel tried to open a file.
enum Loaded {
    Lines(Vec<SharedString>),
    Failed(SharedString),
}

#[derive(Default)]
pub struct SourcesPanel {
    selected: Option<SourceRef>,
    /// The file currently held in memory, and what reading it produced.
    cache: Option<(SharedString, Loaded)>,
}

impl Default for Loaded {
    fn default() -> Self {
        Loaded::Lines(Vec::new())
    }
}

impl SourcesPanel {
    /// Point the panel at a location. The file is read on the next render.
    pub fn reveal(&mut self, source: SourceRef) {
        self.selected = Some(source);
    }

    /// Every distinct file in the tree, with the lowest line seen for each —
    /// which is what clicking the file in the list jumps to.
    ///
    /// Ordered by directory and then by file name, *not* by the whole path.
    /// Sorting by path interleaves `src/button.rs` with `src/input/text.rs`,
    /// which splits a directory's files around its subdirectories and makes
    /// the same heading appear twice in the list.
    pub fn files(tree: &ProbeTree) -> Vec<(SharedString, u32)> {
        let mut lowest: BTreeMap<SharedString, u32> = BTreeMap::new();
        for node in &tree.nodes {
            if let Some(source) = &node.source {
                lowest
                    .entry(source.file.clone())
                    .and_modify(|line| *line = (*line).min(source.line))
                    .or_insert(source.line);
            }
        }

        let mut files: Vec<(SharedString, u32)> = lowest.into_iter().collect();
        files.sort_by(|(a, _), (b, _)| {
            let (a_dir, a_name) = Self::split(a.as_ref());
            let (b_dir, b_name) = Self::split(b.as_ref());
            a_dir.cmp(b_dir).then(a_name.cmp(b_name))
        });
        files
    }

    /// Split a path into the directory it lives in and its file name, so the
    /// list can group the way a file tree would.
    pub fn split(path: &str) -> (&str, &str) {
        match path.rsplit_once('/') {
            Some((directory, name)) => (directory, name),
            None => ("", path),
        }
    }

    fn load(&mut self) {
        let Some(source) = self.selected.as_ref() else {
            return;
        };
        if self
            .cache
            .as_ref()
            .is_some_and(|(path, _)| path == &source.file)
        {
            return;
        }

        let path = source.file.clone();
        let loaded = match resolve(path.as_ref()) {
            None => Loaded::Failed(SharedString::from(format!("Cannot open {path}"))),
            Some(resolved) => match std::fs::metadata(&resolved) {
                Err(error) => {
                    Loaded::Failed(SharedString::from(format!("Cannot open {path}: {error}")))
                }
                Ok(metadata) if metadata.len() > MAX_BYTES => Loaded::Failed(SharedString::from(
                    format!("{path} is too large to display"),
                )),
                Ok(_) => match std::fs::read_to_string(&resolved) {
                    Err(error) => {
                        Loaded::Failed(SharedString::from(format!("Cannot read {path}: {error}")))
                    }
                    Ok(text) => Loaded::Lines(
                        text.lines()
                            .map(|line| SharedString::from(line.to_string()))
                            .collect(),
                    ),
                },
            },
        };

        self.cache = Some((path, loaded));
    }

    pub fn render(
        &mut self,
        tree: &ProbeTree,
        window: &mut Window,
        cx: &mut Context<DevTools>,
    ) -> AnyElement {
        let ink = Ink::read(cx);
        let files = Self::files(tree);
        self.load();

        if files.is_empty() && self.selected.is_none() {
            return empty_state(
                "No source locations recorded yet. Select an element to jump to its source.",
                &ink,
            )
            .into_any_element();
        }

        let _ = window;

        div()
            .flex()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(self.nav(&files, &ink, cx))
            .child(self.content(&ink))
            .into_any_element()
    }

    fn nav(
        &self,
        files: &[(SharedString, u32)],
        ink: &Ink,
        cx: &mut Context<DevTools>,
    ) -> AnyElement {
        let mut nav = div()
            .id("devtools-sources-nav")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(NAV_WIDTH + 40.0))
            .h_full()
            .overflow_scroll()
            .bg(ink.chrome)
            .border_r_1()
            .border_color(ink.border)
            .text_size(px(LABEL_SIZE));

        let mut directory: Option<&str> = None;
        for (position, (path, line)) in files.iter().enumerate() {
            let (parent, name) = Self::split(path.as_ref());
            if directory != Some(parent) {
                directory = Some(parent);
                nav = nav.child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(4.0))
                        .h(px(20.0))
                        .w_full()
                        .px(px(8.0))
                        .text_color(ink.dim)
                        .child(div().flex_1().truncate_text().child(SharedString::from(
                            if parent.is_empty() {
                                "/".to_string()
                            } else {
                                parent.to_string()
                            },
                        ))),
                );
            }

            let selected = self
                .selected
                .as_ref()
                .is_some_and(|source| &source.file == path);
            let target = SourceRef::new(path.clone(), *line, 1);
            let hover_bg = ink.hover;
            let fg = if selected {
                ink.selected_text
            } else {
                ink.text
            };

            nav = nav.child(
                div()
                    .id(("devtools-source-file", position))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .flex_none()
                    .h(px(ROW_HEIGHT))
                    .w_full()
                    .pl(px(18.0))
                    .pr(px(8.0))
                    .text_color(fg)
                    .when(selected, |el| el.bg(ink.selected))
                    .when(!selected, |el| el.hover(move |st| st.bg(hover_bg)))
                    .child(glyph(IconName::FileCode, 11.0, fg, cx))
                    .child(
                        div()
                            .flex_1()
                            .truncate_text()
                            .child(SharedString::from(name.to_string())),
                    )
                    .on_click(
                        cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                            this.sources.reveal(target.clone());
                            cx.notify();
                        }),
                    ),
            );
        }

        nav.into_any_element()
    }

    fn content(&self, ink: &Ink) -> AnyElement {
        let Some(source) = self.selected.as_ref() else {
            return empty_state("Select a file", ink).into_any_element();
        };

        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(22.0))
            .w_full()
            .px(px(8.0))
            .bg(ink.chrome)
            .border_b_1()
            .border_color(ink.border)
            .text_size(px(LABEL_SIZE))
            .text_color(ink.dim)
            .child(source.file.clone())
            .child(SharedString::from(format!("Line {}", source.line)));

        let body: AnyElement = match self.cache.as_ref().map(|(_, loaded)| loaded) {
            Some(Loaded::Failed(message)) => div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .w_full()
                .px(px(16.0))
                .text_size(px(12.0))
                .text_color(ink.dim)
                .child(message.clone())
                .into_any_element(),
            Some(Loaded::Lines(lines)) => {
                let mut listing = div()
                    .id("devtools-sources-listing")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_scroll()
                    .bg(ink.content)
                    .font_family(MONO_FAMILY)
                    .text_size(px(MONO_SIZE));

                // A whole file of rows is a lot of elements; show a window
                // around the target line, which is the only part anyone reads.
                let target = source.line.saturating_sub(1) as usize;
                let first = target.saturating_sub(60);
                let last = (target + 120).min(lines.len());

                for (offset, line) in lines[first.min(lines.len())..last].iter().enumerate() {
                    let number = first + offset + 1;
                    let is_target = number == source.line as usize;
                    listing = listing.child(
                        div()
                            .flex()
                            .items_start()
                            .flex_none()
                            .w_full()
                            .when(is_target, |el| el.bg(ink.selected.opacity(0.22)))
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(48.0))
                                    .px(px(6.0))
                                    .text_color(if is_target { ink.accent } else { ink.dim })
                                    .child(SharedString::from(number.to_string())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .pr(px(8.0))
                                    .text_color(ink.text)
                                    .child(line.clone()),
                            ),
                    );
                }

                listing.into_any_element()
            }
            None => div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .w_full()
                .text_color(ink.dim)
                .child(SharedString::new_static("Loading…"))
                .into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .child(header)
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devtools::probe;

    #[test]
    fn a_path_splits_into_directory_and_name() {
        assert_eq!(
            SourcesPanel::split("crates/guise/src/button.rs"),
            ("crates/guise/src", "button.rs")
        );
        assert_eq!(SourcesPanel::split("main.rs"), ("", "main.rs"));
    }

    #[test]
    fn files_are_deduplicated_to_their_lowest_line() {
        probe::set_enabled(false);
        probe::set_enabled(true);
        probe::test_record("Root", || {});
        probe::begin_frame_unclaimed();

        // The recorder's test driver reports no source, so build the map from a
        // tree assembled by hand instead.
        let mut tree = probe::tree();
        tree.nodes[0].source = Some(SourceRef::new("a.rs", 40, 1));
        let mut second = tree.nodes[0].clone();
        second.source = Some(SourceRef::new("a.rs", 12, 1));
        second.key = "Root[1]".into();
        tree.nodes.push(second);

        let files = SourcesPanel::files(&tree);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], (SharedString::from("a.rs"), 12));
    }

    #[test]
    fn files_group_by_directory_then_name() {
        probe::set_enabled(false);
        probe::set_enabled(true);
        probe::test_record("Root", || {});
        probe::begin_frame_unclaimed();

        let mut tree = probe::tree();
        let template = tree.nodes[0].clone();
        tree.nodes.clear();
        // `src/b.rs` sorts after `src/sub/a.rs` by full path; grouping has to
        // keep the two `src` files together regardless.
        for (index, path) in ["src/sub/a.rs", "src/b.rs", "src/a.rs"]
            .into_iter()
            .enumerate()
        {
            let mut node = template.clone();
            node.key = SharedString::from(format!("Node[{index}]"));
            node.source = Some(SourceRef::new(path, 1, 1));
            tree.nodes.push(node);
        }

        let files = SourcesPanel::files(&tree);
        let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_ref()).collect();
        assert_eq!(paths, vec!["src/a.rs", "src/b.rs", "src/sub/a.rs"]);
    }

    #[test]
    fn a_workspace_relative_path_resolves_from_a_package_directory() {
        // `file!()` is relative to the workspace root while the test process
        // runs in the package, which is exactly the mismatch `resolve` exists
        // to paper over.
        assert!(resolve(file!()).is_some());
        assert!(resolve("definitely/not/a/real/path.rs").is_none());
    }

    #[test]
    fn an_absolute_path_is_used_as_given() {
        let absolute = resolve(file!()).expect("this file resolves");
        assert_eq!(
            resolve(absolute.to_str().unwrap()).as_deref(),
            Some(absolute.as_path())
        );
    }

    #[test]
    fn a_missing_file_reports_rather_than_panicking() {
        let mut panel = SourcesPanel::default();
        panel.reveal(SourceRef::new("/definitely/not/here.rs", 3, 1));
        panel.load();

        match panel.cache.as_ref().map(|(_, loaded)| loaded) {
            Some(Loaded::Failed(message)) => assert!(message.contains("Cannot open")),
            _ => panic!("a missing file should load as a failure"),
        }
    }

    #[test]
    fn an_existing_file_loads_its_lines() {
        let mut panel = SourcesPanel::default();
        panel.reveal(SourceRef::new(file!(), line!(), 1));
        panel.load();

        match panel.cache.as_ref().map(|(_, loaded)| loaded) {
            Some(Loaded::Lines(lines)) => {
                assert!(!lines.is_empty());
                assert!(lines.iter().any(|line| line.contains("SourcesPanel")));
            }
            _ => panic!("this test's own source should be readable"),
        }
    }

    #[test]
    fn loading_twice_does_not_reread_the_same_file() {
        let mut panel = SourcesPanel::default();
        panel.reveal(SourceRef::new(file!(), 1, 1));
        panel.load();
        let first = match panel.cache.as_ref().map(|(_, loaded)| loaded) {
            Some(Loaded::Lines(lines)) => lines.len(),
            _ => panic!("expected the file to load"),
        };

        // A second reveal into the same file at a different line must reuse the
        // cache rather than paying for the read again.
        panel.reveal(SourceRef::new(file!(), 99, 1));
        panel.load();
        let second = match panel.cache.as_ref().map(|(_, loaded)| loaded) {
            Some(Loaded::Lines(lines)) => lines.len(),
            _ => panic!("expected the cache to survive"),
        };

        assert_eq!(first, second);
    }
}
