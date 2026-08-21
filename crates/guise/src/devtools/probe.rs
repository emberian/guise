//! The element-tree recorder behind the Elements panel.
//!
//! gpui knows which element the pointer is over — that is what
//! `Window::toggle_inspector` picking gives you — but it will not enumerate a
//! tree: `inspector_hitboxes` is crate-private and only ever holds one frame of
//! whatever happened to be under the cursor. A DOM-style outline has to be
//! recorded by the thing being inspected.
//!
//! So `guise` records its own. [`Probed::probe`] wraps a component's root
//! element in a pass-through [`Probe`] that pushes a node on the way into
//! `prepaint` and pops it on the way out. gpui prepaints depth-first, so the
//! push/pop pairs nest exactly like the element tree does, and the arena that
//! falls out is the tree the panel renders.
//!
//! Two properties make this affordable to leave in every component:
//!
//! * It is off unless the inspector is open ([`set_enabled`]), and off means
//!   two boolean checks per wrapped element per frame.
//! * It never allocates while off — attributes are dropped at the setter.
//!
//! Every window on a thread shares one recorder, so an inspector claims the
//! window it renders in ([`begin_frame`]) and elements prepainting in any
//! other window are skipped. Without that, an app with a second window open
//! records both trees into one and the inspector shows a tree its panels
//! cannot explain.
//!
//! The recorder always runs one frame behind: an entity's `render` happens
//! during `request_layout`, before anything has prepainted, so the panel reads
//! the tree the *previous* frame built. [`begin_frame`] is what rotates them.

use std::cell::RefCell;
use std::panic::Location;

use gpui::{
    App, Bounds, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels,
    SharedString, StyleRefinement, Styled, Window, WindowId,
};

use super::state::SourceRef;

/// One recorded element: a row in the Elements tree.
#[derive(Debug, Clone)]
pub struct ProbeNode {
    /// The component name, rendered as the tag: `Button` shows as `<Button>`.
    pub name: SharedString,
    /// Attributes shown inline after the tag, as a DOM node shows its
    /// attributes. Ordered as the component declared them.
    pub attrs: Vec<(SharedString, SharedString)>,
    /// The gpui element id, when the element has one.
    pub element_id: Option<SharedString>,
    /// Where the component was constructed, for the Node panel and Sources.
    pub source: Option<SourceRef>,
    /// Laid-out bounds, captured during prepaint. This is what the highlight
    /// overlay and the box model read.
    pub bounds: Bounds<Pixels>,
    /// The element's own style, snapshotted before it was laid out. Boxed
    /// because a `StyleRefinement` is large and most of a tree's memory would
    /// otherwise be styles nobody has selected.
    pub style: Option<Box<StyleRefinement>>,
    /// Index of the parent in the arena, or `None` for a root.
    pub parent: Option<usize>,
    /// Indices of the children, in paint order.
    pub children: Vec<usize>,
    /// Nesting depth, precomputed for the tree's indentation.
    pub depth: usize,
    /// A path key that survives across frames, so a selection outlives the
    /// frame it was made in: parent path + tag + sibling ordinal.
    pub key: SharedString,
}

impl ProbeNode {
    /// Whether the node has no children — a leaf renders as `<Tag />`.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

/// A recorded tree: a flat arena plus its roots, in the order they prepainted.
#[derive(Debug, Clone, Default)]
pub struct ProbeTree {
    pub nodes: Vec<ProbeNode>,
    pub roots: Vec<usize>,
}

impl ProbeTree {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn get(&self, index: usize) -> Option<&ProbeNode> {
        self.nodes.get(index)
    }

    /// Find a node by the stable key a previous frame handed out.
    pub fn find(&self, key: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.key.as_ref() == key)
    }

    /// The chain from the root down to `index`, which is what the Elements
    /// panel's breadcrumb bar shows.
    pub fn ancestry(&self, index: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut cursor = Some(index);
        while let Some(i) = cursor {
            chain.push(i);
            cursor = self.nodes.get(i).and_then(|n| n.parent);
        }
        chain.reverse();
        chain
    }

    /// The deepest node whose bounds contain `point`, searched in reverse paint
    /// order so the topmost element wins — the same rule as hit testing.
    pub fn hit(&self, point: gpui::Point<Pixels>) -> Option<usize> {
        let mut best: Option<(usize, usize)> = None;
        for (index, node) in self.nodes.iter().enumerate() {
            if node.bounds.contains(&point) {
                let deeper = best.is_none_or(|(_, depth)| node.depth >= depth);
                if deeper {
                    best = Some((index, node.depth));
                }
            }
        }
        best.map(|(index, _)| index)
    }
}

/// Recording state. Thread-local rather than a gpui `Global` because element
/// methods run in the hot path of every frame and a thread-local read is a
/// pointer deref, where `App::global` is a hash lookup.
#[derive(Default)]
struct Registry {
    /// How many inspectors are alive.
    ///
    /// A count rather than a flag because instances overlap: replacing a
    /// `DevTools` constructs the new one before dropping the old, so a boolean
    /// would have the old one's `Drop` switch recording off underneath its
    /// replacement.
    recorders: usize,
    /// The tree the current frame is prepainting into.
    building: ProbeTree,
    /// The last completed tree — what panels read.
    current: ProbeTree,
    /// Open ancestors, innermost last.
    stack: Vec<usize>,
    /// The window whose inspector claimed this frame, if one did. `None` means
    /// record every window, which is what a host driving the recorder by hand
    /// through [`set_enabled`] wants.
    window: Option<WindowId>,
}

impl Registry {
    fn is_recording(&self) -> bool {
        self.recorders > 0
    }

    fn clear(&mut self) {
        self.building = ProbeTree::default();
        self.current = ProbeTree::default();
        self.stack.clear();
        self.window = None;
    }
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Turn recording on or off outright, ignoring how many inspectors are alive.
/// For a host driving the recorder by hand, and for tests; an inspector uses
/// [`retain`] and [`release`] instead.
pub fn set_enabled(enabled: bool) {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.recorders = usize::from(enabled);
        if !enabled {
            registry.clear();
        }
    });
}

/// Register an inspector. Recording starts on the first one.
pub(crate) fn retain() {
    REGISTRY.with(|registry| registry.borrow_mut().recorders += 1);
}

/// Drop an inspector. Recording stops, and the recorded tree is released, when
/// the last one goes.
pub(crate) fn release() {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.recorders = registry.recorders.saturating_sub(1);
        if registry.recorders == 0 {
            registry.clear();
        }
    });
}

pub fn is_enabled() -> bool {
    REGISTRY.with(|registry| registry.borrow().recorders > 0)
}

/// Promote the tree the last frame recorded and start a fresh one, claiming
/// this frame for `window`. Called from the inspector's `render`, which runs
/// before any of this frame's prepaints, so elements in every other window
/// this thread draws are skipped for the rest of the frame.
///
/// There is one tree per thread, so two inspectors in two windows take the
/// claim from each other every frame and both come up empty. Showing each of
/// them a tree of both windows would be worse.
pub fn begin_frame(window: &Window) {
    rotate(Some(window.window_handle().window_id()))
}

/// Rotate with no window claimed, so every window records. For the sibling
/// tests, which drive the recorder without standing up a window.
#[cfg(test)]
pub(crate) fn begin_frame_unclaimed() {
    rotate(None)
}

fn rotate(claim: Option<WindowId>) {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if !registry.is_recording() {
            return;
        }
        let previous = std::mem::replace(&mut registry.window, claim);
        // An unbalanced stack would mean an element was pushed and never
        // popped; drop it rather than nesting the next frame under a ghost.
        registry.stack.clear();
        let built = std::mem::take(&mut registry.building);
        // A tree recorded before this window held the claim belongs to some
        // other window, or to no window in particular — the frame an inspector
        // first opens on. Drop it rather than show a tree its panels cannot
        // explain; the next frame records a real one.
        if previous == claim && !built.is_empty() {
            registry.current = built;
        }
    });
}

/// The most recently completed tree.
pub fn tree() -> ProbeTree {
    REGISTRY.with(|registry| registry.borrow().current.clone())
}

/// Run `f` against the current tree without cloning it — for a host that wants
/// to inspect the tree without paying for a copy of it.
pub fn with_tree<R>(f: impl FnOnce(&ProbeTree) -> R) -> R {
    REGISTRY.with(|registry| f(&registry.borrow().current))
}

fn push(meta: &ProbeMeta, window: Option<WindowId>) -> Option<usize> {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if !registry.is_recording() {
            return None;
        }
        // Another window prepainting into a tree this window's inspector
        // claimed. Its elements are not what is being inspected.
        if registry.window.is_some() && registry.window != window {
            return None;
        }

        let parent = registry.stack.last().copied();
        let depth = registry.stack.len();
        let ordinal = match parent {
            Some(parent) => registry.building.nodes[parent].children.len(),
            None => registry.building.roots.len(),
        };
        let key = match parent {
            Some(parent) => format!(
                "{}/{}[{}]",
                registry.building.nodes[parent].key, meta.name, ordinal
            ),
            None => format!("{}[{}]", meta.name, ordinal),
        };

        let index = registry.building.nodes.len();
        registry.building.nodes.push(ProbeNode {
            name: meta.name.clone(),
            attrs: meta.attrs.clone(),
            element_id: None,
            source: meta.source.clone(),
            bounds: Bounds::default(),
            style: meta.style.clone(),
            parent,
            children: Vec::new(),
            depth,
            key: key.into(),
        });

        match parent {
            Some(parent) => registry.building.nodes[parent].children.push(index),
            None => registry.building.roots.push(index),
        }
        registry.stack.push(index);
        Some(index)
    })
}

fn pop(index: usize, bounds: Bounds<Pixels>, element_id: Option<ElementId>) {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if let Some(node) = registry.building.nodes.get_mut(index) {
            node.bounds = bounds;
            node.element_id = element_id.map(|id| SharedString::from(id.to_string()));
        }
        // Pop back to this node's own frame. A child that failed to pop would
        // otherwise leave the stack permanently deeper.
        while let Some(top) = registry.stack.pop() {
            if top == index {
                break;
            }
        }
    });
}

/// Drive the recorder directly. Only for tests in sibling modules that need a
/// tree without standing up a window and a full element pass.
#[cfg(test)]
pub(crate) fn test_record(name: &'static str, children: impl FnOnce()) {
    let meta = ProbeMeta {
        name: SharedString::new_static(name),
        attrs: Vec::new(),
        source: None,
        style: None,
    };
    let index = push(&meta, None);
    children();
    if let Some(index) = index {
        pop(index, Bounds::default(), None);
    }
}

/// What a probe knows about the element it wraps, before it becomes one.
#[derive(Debug, Clone)]
struct ProbeMeta {
    name: SharedString,
    attrs: Vec<(SharedString, SharedString)>,
    source: Option<SourceRef>,
    style: Option<Box<StyleRefinement>>,
}

/// A component's root element, tagged for the Elements panel. Built by
/// [`Probed::probe`].
pub struct Probe<E> {
    inner: E,
    meta: ProbeMeta,
    /// Recording state, sampled once at construction so the attribute setters
    /// can skip their allocations entirely while the inspector is closed.
    recording: bool,
}

impl<E> Probe<E> {
    /// Add an attribute, shown inline after the tag name. Dropped without
    /// allocating when the inspector is closed.
    pub fn attr(mut self, name: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        if self.recording {
            self.meta.attrs.push((name.into(), value.into()));
        }
        self
    }

    /// Add an attribute whose value costs something to build. The closure runs
    /// only while the inspector is recording, so a `format!` in a component's
    /// hot render path is not paid for by every release build.
    pub fn attr_with<V: Into<SharedString>>(
        mut self,
        name: impl Into<SharedString>,
        value: impl FnOnce() -> V,
    ) -> Self {
        if self.recording {
            self.meta.attrs.push((name.into(), value().into()));
        }
        self
    }

    /// Add an attribute only when `value` is set — the usual shape for a
    /// component's optional props.
    pub fn attr_opt(
        self,
        name: impl Into<SharedString>,
        value: Option<impl Into<SharedString>>,
    ) -> Self {
        match value {
            Some(value) => self.attr(name, value),
            None => self,
        }
    }

    /// Add an attribute only when `present`, the way a boolean HTML attribute
    /// either appears bare or not at all.
    pub fn attr_if(self, name: impl Into<SharedString>, present: bool) -> Self {
        if present {
            self.attr(name, "")
        } else {
            self
        }
    }
}

/// Tag any element as a component for the Elements panel.
///
/// Call it last, on the element a component's `render` returns:
///
/// ```ignore
/// impl RenderOnce for Button {
///     fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
///         div()
///             // ...
///             .probe("Button")
///             .attr("variant", self.variant.label())
///     }
/// }
/// ```
///
/// The bound is [`Styled`] rather than plain [`IntoElement`] so the probe can
/// snapshot the element's style on the way past — that snapshot is the whole
/// Styles and Computed sidebar. Every component root is a styled element, so
/// in practice this costs nothing.
pub trait Probed: IntoElement + Styled + Sized {
    /// Wrap this element in a probe named `name`.
    #[track_caller]
    fn probe(mut self, name: impl Into<SharedString>) -> Probe<Self> {
        let recording = is_enabled();
        let style = recording.then(|| Box::new(self.style().clone()));
        // `Location::caller()` has to be read here rather than inside the
        // `then` closure: a closure body is not `#[track_caller]`, so it would
        // resolve to this file instead of to the component that called us.
        let caller = Location::caller();
        Probe {
            inner: self,
            meta: ProbeMeta {
                name: name.into(),
                attrs: Vec::new(),
                source: recording.then(|| SourceRef::from(caller)),
                style,
            },
            recording,
        }
    }
}

impl<E: IntoElement + Styled> Probed for E {}

/// The same, for an element that has no style of its own.
///
/// A handful of components return something already composed — a `Field`, a
/// `deferred(..)` overlay, another component — rather than a styled element.
/// Those have no `StyleRefinement` to hand over; the style that matters belongs
/// to whatever they wrapped, and that reports itself separately. They still
/// belong in the tree, so they probe through here, and their Styles sidebar
/// reads as empty — which is the truth.
pub trait ProbedAny: IntoElement + Sized {
    /// Wrap this element in a probe named `name`, without a style snapshot.
    #[track_caller]
    fn probe_any(self, name: impl Into<SharedString>) -> Probe<Self> {
        let recording = is_enabled();
        let caller = Location::caller();
        Probe {
            inner: self,
            meta: ProbeMeta {
                name: name.into(),
                attrs: Vec::new(),
                source: recording.then(|| SourceRef::from(caller)),
                style: None,
            },
            recording,
        }
    }
}

impl<E: IntoElement> ProbedAny for E {}

impl<E: IntoElement> IntoElement for Probe<E> {
    type Element = ProbeElement<E::Element>;

    fn into_element(self) -> Self::Element {
        ProbeElement {
            inner: self.inner.into_element(),
            meta: self.meta,
            index: None,
        }
    }
}

/// The element half of [`Probe`]: forwards every call to the wrapped element,
/// and brackets `prepaint` with the push/pop that builds the tree.
pub struct ProbeElement<E> {
    inner: E,
    meta: ProbeMeta,
    /// Arena slot claimed during prepaint, carried to the pop.
    index: Option<usize>,
}

impl<E: gpui::Element> IntoElement for ProbeElement<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: gpui::Element> gpui::Element for ProbeElement<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.index = push(&self.meta, Some(window.window_handle().window_id()));
        let state = self
            .inner
            .prepaint(id, inspector_id, bounds, request_layout, window, cx);
        if let Some(index) = self.index {
            pop(index, bounds, self.inner.id());
        }
        state
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner.paint(
            id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recorder is thread-local and these tests drive it directly, so each
    /// one starts from a known state.
    fn reset() {
        set_enabled(false);
        set_enabled(true);
    }

    fn meta(name: &str) -> ProbeMeta {
        ProbeMeta {
            name: SharedString::from(name.to_owned()),
            attrs: Vec::new(),
            source: None,
            style: None,
        }
    }

    fn record(name: &str, children: impl FnOnce()) {
        let index = push(&meta(name), None);
        children();
        if let Some(index) = index {
            pop(index, Bounds::default(), None);
        }
    }

    #[test]
    fn nesting_follows_the_push_pop_pairs() {
        reset();
        record("Stack", || {
            record("Button", || {});
            record("Badge", || {});
        });
        begin_frame_unclaimed();

        let tree = tree();
        assert_eq!(tree.roots, vec![0]);
        assert_eq!(tree.nodes[0].name.as_ref(), "Stack");
        assert_eq!(tree.nodes[0].children, vec![1, 2]);
        assert_eq!(tree.nodes[1].parent, Some(0));
        assert_eq!(tree.nodes[1].depth, 1);
        assert_eq!(tree.nodes[2].name.as_ref(), "Badge");
    }

    #[test]
    fn keys_are_path_plus_sibling_ordinal() {
        reset();
        record("Stack", || {
            record("Button", || {});
            record("Button", || {});
        });
        begin_frame_unclaimed();

        let tree = tree();
        assert_eq!(tree.nodes[0].key.as_ref(), "Stack[0]");
        assert_eq!(tree.nodes[1].key.as_ref(), "Stack[0]/Button[0]");
        assert_eq!(tree.nodes[2].key.as_ref(), "Stack[0]/Button[1]");
        assert_eq!(tree.find("Stack[0]/Button[1]"), Some(2));
    }

    #[test]
    fn a_key_survives_the_next_frame() {
        reset();
        record("Stack", || record("Button", || {}));
        begin_frame_unclaimed();
        let before = tree().find("Stack[0]/Button[0]");

        record("Stack", || record("Button", || {}));
        begin_frame_unclaimed();
        let after = tree().find("Stack[0]/Button[0]");

        assert_eq!(before, after);
        assert!(after.is_some());
    }

    #[test]
    fn ancestry_runs_root_first() {
        reset();
        record("AppShell", || record("Stack", || record("Button", || {})));
        begin_frame_unclaimed();

        let tree = tree();
        let button = tree.find("AppShell[0]/Stack[0]/Button[0]").unwrap();
        let names: Vec<_> = tree
            .ancestry(button)
            .into_iter()
            .map(|i| tree.nodes[i].name.to_string())
            .collect();
        assert_eq!(names, vec!["AppShell", "Stack", "Button"]);
    }

    #[test]
    fn multiple_roots_are_kept_in_order() {
        reset();
        record("AppShell", || {});
        record("Modal", || {});
        begin_frame_unclaimed();

        let tree = tree();
        assert_eq!(tree.roots, vec![0, 1]);
        assert_eq!(tree.nodes[1].key.as_ref(), "Modal[1]");
    }

    #[test]
    fn overlapping_inspectors_keep_recording_until_the_last_one_goes() {
        set_enabled(false);
        assert!(!is_enabled());

        retain();
        assert!(is_enabled());

        // Replacing an inspector constructs the new one before dropping the
        // old; a flag would have the old one's `Drop` switch recording off
        // underneath its replacement.
        retain();
        release();
        assert!(is_enabled());

        release();
        assert!(!is_enabled());
    }

    #[test]
    fn an_unbalanced_release_cannot_underflow() {
        set_enabled(false);
        release();
        release();
        retain();
        assert!(is_enabled());
        release();
        assert!(!is_enabled());
    }

    #[test]
    fn the_recorded_tree_is_released_with_the_last_inspector() {
        reset();
        record("Stack", || {});
        begin_frame_unclaimed();
        assert!(!tree().is_empty());

        release();
        assert!(tree().is_empty());
    }

    #[test]
    fn nothing_records_while_disabled() {
        reset();
        set_enabled(false);
        record("Stack", || record("Button", || {}));
        begin_frame_unclaimed();

        assert!(tree().is_empty());
    }

    #[test]
    fn hit_testing_picks_the_deepest_containing_node() {
        reset();
        let outer = push(&meta("Stack"), None).unwrap();
        let inner = push(&meta("Button"), None).unwrap();
        pop(
            inner,
            Bounds {
                origin: gpui::point(gpui::px(10.0), gpui::px(10.0)),
                size: gpui::size(gpui::px(50.0), gpui::px(20.0)),
            },
            None,
        );
        pop(
            outer,
            Bounds {
                origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
                size: gpui::size(gpui::px(200.0), gpui::px(100.0)),
            },
            None,
        );
        begin_frame_unclaimed();

        let tree = tree();
        let hit = tree
            .hit(gpui::point(gpui::px(20.0), gpui::px(15.0)))
            .unwrap();
        assert_eq!(tree.nodes[hit].name.as_ref(), "Button");

        let outside = tree
            .hit(gpui::point(gpui::px(150.0), gpui::px(80.0)))
            .unwrap();
        assert_eq!(tree.nodes[outside].name.as_ref(), "Stack");

        assert!(tree
            .hit(gpui::point(gpui::px(400.0), gpui::px(400.0)))
            .is_none());
    }

    #[test]
    fn an_unbalanced_stack_does_not_leak_into_the_next_frame() {
        reset();
        // Push without popping, as a panicking prepaint would leave things.
        push(&meta("Orphan"), None);
        begin_frame_unclaimed();
        record("Stack", || {});
        begin_frame_unclaimed();

        let tree = tree();
        assert_eq!(tree.roots.len(), 1);
        assert_eq!(tree.nodes[tree.roots[0]].name.as_ref(), "Stack");
        assert_eq!(tree.nodes[tree.roots[0]].depth, 0);
    }
}
