//! The Elements panel: the component tree, and the sidebar that explains the
//! selected node.
//!
//! This is Safari's Elements tab with `<div>` swapped for `<Button>`. The tree
//! comes from [`super::probe`] — every `guise` component tags its root, so the
//! outline is the component hierarchy rather than a wall of anonymous
//! containers, which is the more useful reading of the same structure.
//!
//! The sidebar is read-only by design. A probe node is a snapshot taken during
//! prepaint; writing to it would edit a copy and change nothing on screen. Live
//! style editing is a different mechanism entirely — gpui's own element picker,
//! wired up in [`super::install`] — and pretending otherwise here would be a
//! worse lie than the missing feature.

use std::collections::HashSet;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, Hsla, SharedString, Window};

use super::probe::{ProbeNode, ProbeTree};
use super::shell::{
    cell, disclosure, elide, empty_state, filter_pill, glyph, hairline, hairline_v, kv_row,
    section_header, Ink, LABEL_SIZE, MONO_SIZE, ROW_HEIGHT, SIDEBAR_WIDTH,
};
use super::styles::{box_model, declarations, BoxModel, Declaration};
use super::DevTools;
use crate::icon::IconName;
use crate::style::MONO_FAMILY;

/// The sidebar tabs, in Safari's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElementsSidebar {
    #[default]
    Styles,
    Computed,
    Node,
}

impl ElementsSidebar {
    fn label(self) -> &'static str {
        match self {
            ElementsSidebar::Styles => "Styles",
            ElementsSidebar::Computed => "Computed",
            ElementsSidebar::Node => "Node",
        }
    }

    const ALL: [ElementsSidebar; 3] = [
        ElementsSidebar::Styles,
        ElementsSidebar::Computed,
        ElementsSidebar::Node,
    ];
}

/// Panel state: what is selected, what is folded, which sidebar is showing.
#[derive(Default)]
pub struct ElementsPanel {
    /// The selected node's stable key, which is what survives a re-render.
    pub selected: Option<SharedString>,
    /// Folded subtrees. Stored as the exception so a newly appearing node is
    /// expanded, matching how Safari reveals new DOM.
    collapsed: HashSet<SharedString>,
    pub(crate) sidebar: ElementsSidebar,
}

impl ElementsPanel {
    pub fn select(&mut self, key: impl Into<SharedString>) {
        self.selected = Some(key.into());
    }

    pub fn toggle(&mut self, key: &SharedString) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.clone());
        }
    }

    fn is_collapsed(&self, key: &SharedString) -> bool {
        self.collapsed.contains(key)
    }

    /// Expand every ancestor of `key` so a selection made elsewhere — the
    /// element picker, a Console source link — is actually on screen.
    pub fn reveal(&mut self, tree: &ProbeTree, key: &SharedString) {
        if let Some(index) = tree.find(key) {
            for ancestor in tree.ancestry(index) {
                self.collapsed.remove(&tree.nodes[ancestor].key);
            }
        }
        self.selected = Some(key.clone());
    }

    /// The selected node, if it is still in the current tree. A node can vanish
    /// between frames — a menu closes, a list scrolls — and the panel simply
    /// shows nothing rather than holding a stale copy.
    pub fn selected_node<'a>(&self, tree: &'a ProbeTree) -> Option<&'a ProbeNode> {
        let key = self.selected.as_ref()?;
        tree.find(key).and_then(|index| tree.get(index))
    }

    /// Flatten the tree into rows, honouring folds. Each row is
    /// `(index, is_closing_tag)`; a closing tag is the `</Stack>` line Safari
    /// prints under an expanded node's children.
    fn rows(&self, tree: &ProbeTree) -> Vec<(usize, bool)> {
        let mut rows = Vec::with_capacity(tree.len());
        for root in &tree.roots {
            self.push_rows(tree, *root, &mut rows);
        }
        rows
    }

    fn push_rows(&self, tree: &ProbeTree, index: usize, rows: &mut Vec<(usize, bool)>) {
        let Some(node) = tree.get(index) else {
            return;
        };
        rows.push((index, false));
        if node.is_leaf() || self.is_collapsed(&node.key) {
            return;
        }
        for child in &node.children {
            self.push_rows(tree, *child, rows);
        }
        rows.push((index, true));
    }

    pub fn render(
        &self,
        tree: &ProbeTree,
        window: &mut Window,
        cx: &mut Context<DevTools>,
    ) -> AnyElement {
        let ink = Ink::read(cx);

        if tree.is_empty() {
            return div()
                .flex()
                .flex_1()
                .min_h(px(0.0))
                .child(empty_state(
                    "No elements recorded. Components report themselves while the inspector is open.",
                    &ink,
                ))
                .into_any_element();
        }

        div()
            .flex()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(self.tree_column(tree, &ink, cx))
            .child(hairline_v(&ink))
            .child(self.sidebar_column(tree, &ink, window, cx))
            .into_any_element()
    }

    /// The tree, plus the breadcrumb bar Safari pins under it.
    fn tree_column(&self, tree: &ProbeTree, ink: &Ink, cx: &mut Context<DevTools>) -> AnyElement {
        let rows = self.rows(tree);
        let mut list = div()
            .id("devtools-elements-tree")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_scroll()
            .bg(ink.content)
            .font_family(MONO_FAMILY)
            .text_size(px(MONO_SIZE));

        for (index, closing) in rows {
            let Some(node) = tree.get(index) else {
                continue;
            };
            list = list.child(self.row(node, index, closing, ink, cx));
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .child(list)
            .child(hairline(ink))
            .child(self.breadcrumbs(tree, ink, cx))
            .into_any_element()
    }

    /// One markup line: `▶ <Button variant="filled">`.
    fn row(
        &self,
        node: &ProbeNode,
        index: usize,
        closing: bool,
        ink: &Ink,
        cx: &mut Context<DevTools>,
    ) -> AnyElement {
        let selected = self.selected.as_ref() == Some(&node.key);
        let expandable = !node.is_leaf() && !closing;
        let expanded = !self.is_collapsed(&node.key);
        let indent = node.depth as f32 * 12.0 + 6.0;
        let text_color = if selected {
            ink.selected_text
        } else {
            ink.text
        };
        let punct = if selected {
            ink.selected_text
        } else {
            ink.punct
        };
        let tag_color = if selected { ink.selected_text } else { ink.tag };
        let hover_bg = ink.hover;

        let key_for_click = node.key.clone();
        let key_for_toggle = node.key.clone();

        let mut markup = div()
            .flex()
            .items_center()
            .gap(px(0.0))
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .whitespace_nowrap()
            .child(div().flex_none().text_color(punct).child(if closing {
                SharedString::new_static("</")
            } else {
                SharedString::new_static("<")
            }))
            .child(
                div()
                    .flex_none()
                    .text_color(tag_color)
                    .child(node.name.clone()),
            );

        if !closing {
            for (name, value) in &node.attrs {
                markup = markup.child(
                    div()
                        .flex()
                        .flex_none()
                        .child(div().text_color(punct).child(SharedString::new_static(" ")))
                        .child(
                            div()
                                .text_color(if selected {
                                    ink.selected_text
                                } else {
                                    ink.attr
                                })
                                .child(name.clone()),
                        )
                        .when(!value.is_empty(), |el| {
                            el.child(div().text_color(punct).child(SharedString::new_static("=")))
                                .child(
                                    div()
                                        .text_color(if selected {
                                            ink.selected_text
                                        } else {
                                            ink.value
                                        })
                                        .child(SharedString::from(format!("\"{value}\""))),
                                )
                        }),
                );
            }
        }

        markup = markup.child(div().flex_none().text_color(punct).child(
            if !closing && node.is_leaf() {
                SharedString::new_static(" />")
            } else {
                SharedString::new_static(">")
            },
        ));

        div()
            .id(("devtools-element-row", index * 2 + closing as usize))
            .flex()
            .items_center()
            .flex_none()
            .h(px(ROW_HEIGHT))
            .w_full()
            .pl(px(indent))
            .pr(px(6.0))
            .text_color(text_color)
            .when(selected, |el| el.bg(ink.selected))
            .when(!selected, |el| el.hover(move |st| st.bg(hover_bg)))
            .child(
                div()
                    .id(("devtools-element-twisty", index * 2 + closing as usize))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(px(14.0))
                    .h(px(ROW_HEIGHT))
                    .child(if expandable {
                        disclosure(Some(expanded), ink, cx)
                    } else {
                        disclosure(None, ink, cx)
                    })
                    .on_click(
                        cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                            this.elements.toggle(&key_for_toggle);
                            cx.notify();
                        }),
                    ),
            )
            .child(markup)
            .on_click(
                cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                    this.elements.select(key_for_click.clone());
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// The path from the root to the selection, along the bottom edge.
    fn breadcrumbs(&self, tree: &ProbeTree, ink: &Ink, cx: &mut Context<DevTools>) -> AnyElement {
        let mut bar = div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(22.0))
            .w_full()
            .px(px(6.0))
            .gap(px(2.0))
            .bg(ink.chrome)
            .text_size(px(LABEL_SIZE))
            .overflow_hidden();

        let Some(index) = self.selected.as_ref().and_then(|key| tree.find(key)) else {
            return bar
                .child(
                    div()
                        .text_color(ink.dim)
                        .child(SharedString::new_static("Select an element")),
                )
                .into_any_element();
        };

        let chain = tree.ancestry(index);
        let last = chain.len().saturating_sub(1);
        for (position, node_index) in chain.into_iter().enumerate() {
            let Some(node) = tree.get(node_index) else {
                continue;
            };
            if position > 0 {
                bar = bar.child(
                    div()
                        .flex_none()
                        .text_color(ink.dim)
                        .child(SharedString::new_static("›")),
                );
            }
            let key = node.key.clone();
            let is_last = position == last;
            let hover_bg = ink.hover;
            bar = bar.child(
                div()
                    .id(("devtools-crumb", node_index))
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(17.0))
                    .px(px(5.0))
                    .rounded(px(4.0))
                    .text_color(if is_last { ink.text } else { ink.dim })
                    .hover(move |st| st.bg(hover_bg))
                    .child(node.name.clone())
                    .on_click(
                        cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                            this.elements.select(key.clone());
                            cx.notify();
                        }),
                    ),
            );
        }

        bar.into_any_element()
    }

    fn sidebar_column(
        &self,
        tree: &ProbeTree,
        ink: &Ink,
        window: &mut Window,
        cx: &mut Context<DevTools>,
    ) -> AnyElement {
        let mut tabs = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .h(px(26.0))
            .px(px(8.0))
            .w_full()
            .bg(ink.chrome)
            .border_b_1()
            .border_color(ink.border);

        for tab in ElementsSidebar::ALL {
            tabs = tabs.child(
                filter_pill(
                    ("devtools-elements-sidebar", tab as usize),
                    tab.label(),
                    self.sidebar == tab,
                    ink,
                )
                .on_click(cx.listener(
                    move |this: &mut DevTools, _event, _window, cx| {
                        this.elements.sidebar = tab;
                        cx.notify();
                    },
                )),
            );
        }

        let body = match self.selected_node(tree) {
            None => empty_state("No element selected", ink).into_any_element(),
            Some(node) => match self.sidebar {
                ElementsSidebar::Styles => styles_view(node, ink, cx),
                ElementsSidebar::Computed => computed_view(node, ink, window, cx),
                ElementsSidebar::Node => node_view(node, ink, cx),
            },
        };

        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .bg(ink.content)
            .child(tabs)
            .child(
                div()
                    .id("devtools-elements-sidebar-body")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_scroll()
                    .child(body),
            )
            .into_any_element()
    }
}

/// A `property: value;` line, with a swatch when the value is a color.
fn declaration_row(declaration: &Declaration, ink: &Ink) -> AnyElement {
    div()
        .flex()
        .items_start()
        .w_full()
        .pl(px(20.0))
        .pr(px(8.0))
        .py(px(1.0))
        .font_family(MONO_FAMILY)
        .text_size(px(MONO_SIZE))
        .child(
            div()
                .flex_none()
                .text_color(ink.property)
                .child(declaration.property.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_color(ink.punct)
                .child(SharedString::new_static(": ")),
        )
        .when_some(declaration.color, |el, color| {
            el.child(
                div()
                    .flex_none()
                    .w(px(9.0))
                    .h(px(9.0))
                    .mt(px(3.0))
                    .mr(px(4.0))
                    .rounded(px(2.0))
                    .border_1()
                    .border_color(ink.border)
                    .bg(color),
            )
        })
        .child(
            div()
                .flex_1()
                .text_color(ink.value)
                .child(declaration.value.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_color(ink.punct)
                .child(SharedString::new_static(";")),
        )
        .into_any_element()
}

/// The Styles sidebar: one rule block, headed by the component as its selector.
fn styles_view(node: &ProbeNode, ink: &Ink, _cx: &mut Context<DevTools>) -> AnyElement {
    let Some(style) = node.style.as_ref() else {
        return empty_state("This element reported no style", ink).into_any_element();
    };
    let declarations = declarations(style);

    let mut block = div()
        .flex()
        .flex_col()
        .w_full()
        .py(px(4.0))
        .font_family(MONO_FAMILY)
        .text_size(px(MONO_SIZE))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .px(px(8.0))
                .child(
                    div()
                        .flex()
                        .child(
                            div()
                                .text_color(ink.tag)
                                .child(SharedString::from(node.name.to_string())),
                        )
                        .child(
                            div()
                                .text_color(ink.punct)
                                .child(SharedString::new_static(" {")),
                        ),
                )
                .when_some(node.source.as_ref(), |el, source| {
                    el.child(
                        div()
                            .text_size(px(LABEL_SIZE))
                            .text_color(ink.dim)
                            .child(SharedString::from(source.short())),
                    )
                }),
        );

    if declarations.is_empty() {
        block = block.child(
            div()
                .pl(px(20.0))
                .text_color(ink.dim)
                .child(SharedString::new_static("/* no declarations */")),
        );
    }
    for declaration in &declarations {
        block = block.child(declaration_row(declaration, ink));
    }

    block = block.child(
        div()
            .px(px(8.0))
            .text_color(ink.punct)
            .child(SharedString::new_static("}")),
    );

    div()
        .flex()
        .flex_col()
        .w_full()
        .child(section_header(
            SharedString::from(format!("{} — {} rules", node.name, 1)),
            ink,
        ))
        .child(block)
        .into_any_element()
}

/// The Computed sidebar: the box model diagram, then every declaration sorted
/// by name — the shape Safari's Computed pane has.
fn computed_view(
    node: &ProbeNode,
    ink: &Ink,
    window: &mut Window,
    _cx: &mut Context<DevTools>,
) -> AnyElement {
    let rem_size = window.rem_size();
    let model = node
        .style
        .as_ref()
        .map(|style| box_model(style, node.bounds.size, rem_size))
        .unwrap_or(BoxModel {
            width: f32::from(node.bounds.size.width),
            height: f32::from(node.bounds.size.height),
            ..BoxModel::default()
        });

    let mut sorted = node
        .style
        .as_ref()
        .map(|style| declarations(style))
        .unwrap_or_default();
    sorted.sort_by(|a, b| a.property.cmp(&b.property));

    let mut properties = div().flex().flex_col().w_full().pb(px(6.0));
    for declaration in &sorted {
        properties = properties.child(declaration_row(declaration, ink));
    }

    div()
        .flex()
        .flex_col()
        .w_full()
        .child(section_header("Box Model", ink))
        .child(box_model_view(&model, ink))
        .child(section_header("Properties", ink))
        .child(properties)
        .into_any_element()
}

/// Nested boxes labelled with their edge values, outermost first — margin,
/// border, padding, content.
fn box_model_view(model: &BoxModel, ink: &Ink) -> AnyElement {
    let (content_width, content_height) = model.content();

    let band = |label: &'static str,
                color: Hsla,
                top: f32,
                right: f32,
                bottom: f32,
                left: f32,
                inner: AnyElement| {
        div()
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .bg(color)
            .border_1()
            .border_color(ink.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .px(px(4.0))
                    .child(
                        // The band is a saturated fill, so the dim text color
                        // this would otherwise use disappears into it.
                        div()
                            .text_size(px(9.0))
                            .text_color(ink.text)
                            .child(SharedString::new_static(label)),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(ink.text)
                            .child(SharedString::from(number(top))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .gap(px(4.0))
                    .px(px(4.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(9.0))
                            .text_color(ink.text)
                            .child(SharedString::from(number(left))),
                    )
                    .child(inner)
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(9.0))
                            .text_color(ink.text)
                            .child(SharedString::from(number(right))),
                    ),
            )
            .child(
                div().flex().justify_center().w_full().child(
                    div()
                        .text_size(px(9.0))
                        .text_color(ink.text)
                        .child(SharedString::from(number(bottom))),
                ),
            )
    };

    let content = div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .h(px(30.0))
        .bg(ink.box_content)
        .border_1()
        .border_color(ink.border)
        .text_size(px(10.0))
        .text_color(ink.text)
        .child(SharedString::from(format!(
            "{} × {}",
            number(content_width),
            number(content_height)
        )))
        .into_any_element();

    let padding = band(
        "padding",
        ink.box_padding,
        model.padding.top,
        model.padding.right,
        model.padding.bottom,
        model.padding.left,
        content,
    )
    .into_any_element();

    let border = band(
        "border",
        ink.box_border,
        model.border.top,
        model.border.right,
        model.border.bottom,
        model.border.left,
        padding,
    )
    .into_any_element();

    let margin = band(
        "margin",
        ink.box_margin,
        model.margin.top,
        model.margin.right,
        model.margin.bottom,
        model.margin.left,
        border,
    );

    div()
        .flex()
        .flex_col()
        .w_full()
        .p(px(10.0))
        .font_family(MONO_FAMILY)
        .child(margin)
        .into_any_element()
}

/// `0` reads better than `0.00`, and a fractional pixel is worth seeing.
fn number(value: f32) -> String {
    if (value - value.round()).abs() < 0.01 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

/// The Node sidebar: identity, geometry, and the reported attributes.
fn node_view(node: &ProbeNode, ink: &Ink, cx: &mut Context<DevTools>) -> AnyElement {
    let mut identity = div().flex().flex_col().w_full().py(px(4.0));
    identity = identity.child(kv_row("Component", node.name.clone(), ink));
    identity = identity.child(kv_row("Path", node.key.clone(), ink));
    if let Some(id) = &node.element_id {
        identity = identity.child(kv_row("Element ID", id.clone(), ink));
    }
    if let Some(source) = &node.source {
        let target = source.clone();
        identity = identity.child(
            div()
                .id("devtools-node-source")
                .child(kv_row(
                    "Source",
                    SharedString::from(
                        elide(source.file.as_ref(), 34) + &format!(":{}", source.line),
                    ),
                    ink,
                ))
                .on_click(
                    cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                        this.reveal_source(target.clone(), cx);
                    }),
                ),
        );
    }

    let bounds = node.bounds;
    let mut geometry = div().flex().flex_col().w_full().py(px(4.0));
    geometry = geometry.child(kv_row(
        "Position",
        SharedString::from(format!(
            "{}, {}",
            number(f32::from(bounds.origin.x)),
            number(f32::from(bounds.origin.y))
        )),
        ink,
    ));
    geometry = geometry.child(kv_row(
        "Size",
        SharedString::from(format!(
            "{} × {}",
            number(f32::from(bounds.size.width)),
            number(f32::from(bounds.size.height))
        )),
        ink,
    ));
    geometry = geometry.child(kv_row(
        "Depth",
        SharedString::from(node.depth.to_string()),
        ink,
    ));

    let mut attributes = div().flex().flex_col().w_full().py(px(4.0));
    if node.attrs.is_empty() {
        attributes = attributes.child(
            div()
                .px(px(8.0))
                .py(px(2.0))
                .text_size(px(LABEL_SIZE))
                .text_color(ink.dim)
                .child(SharedString::new_static("None reported")),
        );
    }
    for (name, value) in &node.attrs {
        attributes = attributes.child(kv_row(
            name.clone(),
            if value.is_empty() {
                SharedString::new_static("true")
            } else {
                value.clone()
            },
            ink,
        ));
    }

    div()
        .flex()
        .flex_col()
        .w_full()
        .child(section_header("Identity", ink))
        .child(identity)
        .child(section_header("Geometry", ink))
        .child(geometry)
        .child(section_header("Attributes", ink))
        .child(attributes)
        .into_any_element()
}

/// The Layers panel reuses the tree: gpui has no compositing layers to show, so
/// what is genuinely useful is paint order and geometry, which is what this
/// lists — deepest-painting last, as the compositor would.
pub fn layers_view(
    tree: &ProbeTree,
    selected: Option<&SharedString>,
    ink: &Ink,
    cx: &mut Context<DevTools>,
) -> AnyElement {
    if tree.is_empty() {
        return empty_state("No elements recorded", ink).into_any_element();
    }

    let mut rows: Vec<&ProbeNode> = tree.nodes.iter().collect();
    rows.sort_by(|a, b| {
        let area_a = f32::from(a.bounds.size.width) * f32::from(a.bounds.size.height);
        let area_b = f32::from(b.bounds.size.width) * f32::from(b.bounds.size.height);
        area_b
            .partial_cmp(&area_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut list = div()
        .id("devtools-layers")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .overflow_scroll()
        .bg(ink.content)
        .font_family(MONO_FAMILY)
        .text_size(px(MONO_SIZE));

    list = list.child(
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(20.0))
            .w_full()
            .bg(ink.chrome)
            .border_b_1()
            .border_color(ink.border)
            .child(cell("Layer", None, ink.dim))
            .child(cell("Depth", Some(52.0), ink.dim))
            .child(cell("Position", Some(90.0), ink.dim))
            .child(cell("Size", Some(90.0), ink.dim))
            .child(cell("Area", Some(78.0), ink.dim)),
    );

    for (position, node) in rows.iter().enumerate() {
        let is_selected = selected == Some(&node.key);
        let key = node.key.clone();
        let hover_bg = ink.hover;
        let area = f32::from(node.bounds.size.width) * f32::from(node.bounds.size.height);
        let text = if is_selected {
            ink.selected_text
        } else {
            ink.text
        };
        let dim = if is_selected {
            ink.selected_text
        } else {
            ink.dim
        };

        list = list.child(
            div()
                .id(("devtools-layer-row", position))
                .flex()
                .items_center()
                .flex_none()
                .h(px(ROW_HEIGHT))
                .w_full()
                .when(is_selected, |el| el.bg(ink.selected))
                .when(!is_selected && position % 2 == 1, |el| el.bg(ink.stripe))
                .when(!is_selected, |el| el.hover(move |st| st.bg(hover_bg)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_1()
                        .min_w(px(0.0))
                        .h_full()
                        .px(px(6.0))
                        .gap(px(5.0))
                        .child(glyph(IconName::Layers, 11.0, dim, cx))
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(text)
                                .child(node.name.clone()),
                        ),
                )
                .child(cell(node.depth.to_string(), Some(52.0), dim))
                .child(cell(
                    format!(
                        "{}, {}",
                        number(f32::from(node.bounds.origin.x)),
                        number(f32::from(node.bounds.origin.y))
                    ),
                    Some(90.0),
                    dim,
                ))
                .child(cell(
                    format!(
                        "{} × {}",
                        number(f32::from(node.bounds.size.width)),
                        number(f32::from(node.bounds.size.height))
                    ),
                    Some(90.0),
                    dim,
                ))
                // Areas run to six digits; a fractional pixel of area is noise.
                .child(cell(
                    format!("{} px²", area.round() as i64),
                    Some(84.0),
                    dim,
                ))
                .on_click(
                    cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                        this.elements.select(key.clone());
                        cx.notify();
                    }),
                ),
        );
    }

    list.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devtools::probe;

    fn tree_of(build: impl FnOnce()) -> ProbeTree {
        probe::set_enabled(false);
        probe::set_enabled(true);
        build();
        probe::begin_frame();
        probe::tree()
    }

    /// Drives the recorder the way `ProbeElement::prepaint` does.
    fn node(name: &'static str, children: impl FnOnce()) {
        probe::test_record(name, children);
    }

    #[test]
    fn a_leaf_produces_one_row_and_a_parent_produces_two() {
        let panel = ElementsPanel::default();
        let tree = tree_of(|| {
            node("Stack", || {
                node("Button", || {});
            });
        });

        let rows = panel.rows(&tree);
        // Stack open, Button (leaf), Stack close.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], (0, false));
        assert_eq!(rows[1], (1, false));
        assert_eq!(rows[2], (0, true));
    }

    #[test]
    fn collapsing_hides_children_and_the_closing_tag() {
        let mut panel = ElementsPanel::default();
        let tree = tree_of(|| {
            node("Stack", || {
                node("Button", || {});
                node("Badge", || {});
            });
        });

        panel.toggle(&tree.nodes[0].key.clone());
        let rows = panel.rows(&tree);
        assert_eq!(rows, vec![(0, false)]);
    }

    #[test]
    fn toggling_twice_restores_the_children() {
        let mut panel = ElementsPanel::default();
        let tree = tree_of(|| node("Stack", || node("Button", || {})));
        let key = tree.nodes[0].key.clone();

        panel.toggle(&key);
        panel.toggle(&key);
        assert_eq!(panel.rows(&tree).len(), 3);
    }

    #[test]
    fn revealing_expands_every_ancestor() {
        let mut panel = ElementsPanel::default();
        let tree = tree_of(|| node("AppShell", || node("Stack", || node("Button", || {}))));
        let leaf = tree.nodes[2].key.clone();

        panel.toggle(&tree.nodes[0].key.clone());
        panel.toggle(&tree.nodes[1].key.clone());
        panel.reveal(&tree, &leaf);

        assert_eq!(panel.selected.as_ref(), Some(&leaf));
        assert!(panel.rows(&tree).iter().any(|(index, _)| *index == 2));
    }

    #[test]
    fn a_selection_that_left_the_tree_resolves_to_nothing() {
        let mut panel = ElementsPanel::default();
        let tree = tree_of(|| node("Stack", || node("Menu", || {})));
        panel.select(tree.nodes[1].key.clone());
        assert!(panel.selected_node(&tree).is_some());

        let without_menu = tree_of(|| node("Stack", || {}));
        assert!(panel.selected_node(&without_menu).is_none());
    }
}
