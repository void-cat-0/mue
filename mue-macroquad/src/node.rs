pub mod graphics;
pub mod layout;
pub mod typography;

pub use graphics::*;
pub use layout::*;
pub use typography::*;

use std::{cell::RefCell, ops::Deref, rc::Rc};

use mue_core::{
    Disposable, Owned,
    prop::Prop,
    scope::Scope,
    signal::{Access, ReadSignal, Signal, signal},
};
use slotmap::{Key, new_key_type};

use crate::{
    event::hit_test::HitTestFn,
    gesture::GestureId,
    hook::NodeHooks,
    layout::OwnedLayout,
    math::{Matrix, Rect, Vector},
    runtime::Runtime,
    style::{FontSize, FontSizeExt, Style, Styleable, StyleableExt},
};

new_key_type! {
    pub(crate) struct NodeId;
}

thread_local! {
    #[allow(clippy::vec_box)]
    static CONTEXT: RefCell<Vec<Box<NodeInner>>> = RefCell::default();
}

pub(crate) struct NodeInner {
    pub scope: Scope,
    pub(crate) retained_scopes: Vec<Scope>,
    pub hooks: NodeHooks,

    pub(crate) layout_id: Option<taffy::NodeId>,
    pub(crate) layout: OwnedLayout,
    // Return true after changing layout inputs so they are recomputed before paint.
    pub(crate) layout_sync: Option<Rc<dyn Fn(Vector) -> bool>>,
    pub(crate) hit_test: Option<HitTestFn>,
    pub(crate) gestures: Vec<GestureId>,
    pub(crate) clip: Option<ReadSignal<Rect>>,
    pub(crate) child_offset: Option<ReadSignal<Vector>>,
    pub(crate) center_on_layout_origin: bool,
    pub(crate) transform: Prop<Matrix>,
    pub(crate) font_size: Option<Prop<FontSize>>,
    pub(crate) computed_font_size: Signal<f32>,

    pub(crate) children: Owned<Children>,
}

impl NodeInner {
    pub fn new(children: Owned<Children>) -> Self {
        Self {
            scope: Scope::new(),
            retained_scopes: Vec::new(),
            hooks: NodeHooks::default(),

            layout_id: None,
            layout: OwnedLayout::default(),
            layout_sync: None,
            hit_test: None,
            gestures: Vec::new(),
            clip: None,
            child_offset: None,
            center_on_layout_origin: false,
            transform: Prop::Static(Matrix::IDENTITY),
            font_size: None,
            computed_font_size: Signal::null(),

            children,
        }
    }

    pub fn with_mut<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        let mut ctx = CONTEXT.with_borrow_mut(|stack| stack.pop().expect("no node context found"));
        let result = f(ctx.as_mut());
        CONTEXT.with_borrow_mut(|stack| stack.push(ctx));
        result
    }

    pub fn register(self) -> NodeId {
        Runtime::with(|rt| rt.nodes.borrow_mut().insert(self))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
/// A borrowed node handle; copying it does not extend the node's lifetime.
pub struct NodeRef {
    pub(crate) id: NodeId,
}

impl NodeRef {
    pub(crate) fn null() -> Self {
        Self { id: NodeId::null() }
    }

    pub(crate) fn sync_layout(&self) -> bool {
        let (sync, size, children) = Runtime::with(|rt| {
            let node = rt.node(self.id);
            let size = node
                .layout_sync
                .as_ref()
                .and_then(|_| node.layout_id)
                .map(|id| {
                    let size = rt.taffy.borrow().layout(id).unwrap().size;
                    Vector::new(size.width, size.height)
                });
            (node.layout_sync.clone(), size, *node.children)
        });
        let mut changed = match (sync, size) {
            (Some(sync), Some(size)) => sync(size),
            _ => false,
        };
        for child in children.get_clone_untracked().iter() {
            changed |= child.sync_layout();
        }
        changed
    }

    pub(crate) fn render(&self, mut origin: Vector) {
        Runtime::with(|rt| {
            let node = rt.node(self.id);
            if let Some(layout_id) = node.layout_id {
                let taffy = rt.taffy.borrow();
                let layout = taffy.layout(layout_id).unwrap();
                let loc = layout.location;
                let size = layout.size;
                origin.x += loc.x;
                origin.y += loc.y;
                if node.center_on_layout_origin {
                    origin -= Vector::new(size.width, size.height) / 2.;
                }
                node.layout
                    .rect
                    .set(Rect::new(origin.x, origin.y, size.width, size.height));
            }

            let transform = node.transform.get();
            drop(node);
            crate::paint::with_transform(transform, || {
                rt.invoke_hook(self.id, |hooks| &mut hooks.render, &());

                let node = rt.node(self.id);
                let clip = node.clip.map(|rect| rect.get());
                if let Some(offset) = node.child_offset {
                    origin -= offset.get();
                }
                let children = *node.children;
                drop(node);
                let draw = || {
                    if !children.is_null() {
                        for child in children.get_clone().iter() {
                            child.render(origin);
                        }
                    }
                    rt.invoke_hook(self.id, |hooks| &mut hooks.after_children, &());
                };
                if let Some(clip) = clip {
                    crate::paint::with_clip(clip, draw);
                } else {
                    draw();
                }
            });
        });
    }
}

/// The node's resolved logical-pixel font size, shared by measurement and paint.
pub fn use_font_size() -> ReadSignal<f32> {
    NodeInner::with_mut(|node| *node.computed_font_size)
}

impl NodeRef {
    pub(crate) fn resolve_font_sizes(&self) {
        fn visit(node: NodeRef, parent: f32, root: Option<f32>) -> bool {
            let (declaration, computed, children) = Runtime::with(|rt| {
                let node = rt.node(node.id);
                (node.font_size, node.computed_font_size, *node.children)
            });
            let size = declaration.map_or(parent, |value| {
                value
                    .get()
                    .resolve(parent, root.unwrap_or(FontSize::DEFAULT_PX))
            });
            let mut changed = computed.get_untracked() != size;
            if changed {
                computed.set(size);
            }
            let root = root.unwrap_or(size);
            for child in children.get_clone().iter() {
                changed |= visit(*child, size, Some(root));
            }
            changed
        }
        // Font subscribers can mount children when the batch flushes. Resolve
        // those additions before measurement rather than one frame later.
        for _ in 0..64 {
            let mut changed = false;
            mue_core::batch(|| changed = visit(*self, FontSize::DEFAULT_PX, None));
            if !changed {
                return;
            }
        }
        panic!("font-size dependencies did not stabilize before layout");
    }
}

impl Disposable for NodeRef {
    fn dispose(&self) {
        let _ = Runtime::try_with(|rt| {
            mue_core::batch(|| {
                let Some(node) = rt.nodes.borrow_mut().remove(self.id) else {
                    return;
                };
                let mut first_error = None;
                let mut dispose = |f: &mut dyn FnMut()| {
                    if let Err(error) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
                        && first_error.is_none()
                    {
                        first_error = Some(error);
                    }
                };
                // The children signal owns the child Nodes through its callback.
                // Disposing it drops those Nodes, including hidden children.
                dispose(&mut || node.children.dispose());
                dispose(&mut || node.scope.dispose());
                for scope in &node.retained_scopes {
                    dispose(&mut || scope.dispose());
                }
                if let Some(layout_id) = node.layout_id {
                    dispose(&mut || {
                        let mut taffy = rt.taffy.borrow_mut();
                        // Taffy's remove does not release the measurement context.
                        taffy.set_node_context(layout_id, None).unwrap();
                        taffy.remove(layout_id).unwrap();
                    });
                }
                for gesture_id in &node.gestures {
                    rt.gestures.borrow_mut().remove(*gesture_id);
                }
                if let Some(error) = first_error {
                    std::panic::resume_unwind(error);
                }
            });
        });
    }
}

#[repr(transparent)]
/// An owning node handle. Dropping it releases its children and reactive scope.
pub struct Node(Owned<NodeRef>);

impl Node {
    pub(crate) fn retain_scope(&self, scope: Scope) {
        Runtime::with(|rt| rt.nodes.borrow_mut()[self.id].retained_scopes.push(scope));
    }

    /// Build a component in a scope retained by its original root node.
    #[doc(hidden)]
    pub fn build_component(f: impl FnOnce() -> Self) -> Self {
        struct ScopeGuard(Option<Scope>);

        impl Drop for ScopeGuard {
            fn drop(&mut self) {
                if let Some(scope) = self.0.take() {
                    scope.dispose();
                }
            }
        }

        let scope = Scope::new();
        let mut guard = ScopeGuard(Some(scope));
        let node = scope.run(f);
        node.retain_scope(scope);
        guard.0 = None;
        node
    }

    /// Build a custom node within its own reactive scope.
    pub fn build(f: impl FnOnce()) -> Self {
        Self::build_with_style(Style::default(), |_style| f())
    }

    /// Build a custom node with layout, paint, and event styles.
    pub fn build_with_style(mut style: Style, f: impl FnOnce(&mut Style)) -> Self {
        let node = NodeInner::new(style.children.take().unwrap_or_else(|| ().into_children()));
        let scope = node.scope;
        CONTEXT.with_borrow_mut(|stack| stack.push(Box::new(node)));
        scope.run(|| {
            crate::animation::bind_style(&mut style);
            NodeInner::with_mut(|node| {
                node.computed_font_size = signal(FontSize::DEFAULT_PX);
                node.font_size = style.font_size;
            });
            // Apply once around the whole subtree, including custom draw hooks.
            NodeInner::with_mut(|node| node.transform = style.take_transform());
            f(&mut style);
        });
        let mut node = CONTEXT.with_borrow_mut(|stack| stack.pop().unwrap());
        if let Some(font_size) = style.font_size.take() {
            node.font_size = Some(font_size);
        }
        let after_children = std::mem::take(&mut node.hooks.after_children);
        node.hooks = style.build_hooks();
        node.hooks.after_children = after_children;
        append_hooks(node.as_mut(), &mut style);
        Self(
            NodeRef {
                id: node.register(),
            }
            .owned(),
        )
    }
}

fn append_hooks(node: &mut NodeInner, style: &mut Style) {
    use crate::gesture;

    Runtime::with(|rt| {
        let mut gestures = rt.gestures.borrow_mut();
        let mut insert = |gesture| node.gestures.push(gestures.insert(gesture));

        if !style.on_tap.is_empty() {
            let mut gesture = Box::new(gesture::TapGesture::default());
            gesture.on_tap = style.take_on_tap();
            insert(gesture);
        }

        if !style.on_long_press.is_empty() {
            let mut gesture = Box::new(gesture::LongPressGesture::default());
            gesture.on_long_press = Rc::new(RefCell::new(style.take_on_long_press()));
            insert(gesture);
        }
        if !style.on_pinch.is_empty() {
            let mut gesture = Box::new(gesture::PinchGesture::default());
            gesture.on_pinch = style.take_on_pinch();
            insert(gesture);
        }

        if !style.on_rotate.is_empty() {
            let mut gesture = Box::new(gesture::RotateGesture::default());
            gesture.on_rotate = style.take_on_rotate();
            insert(gesture);
        }

        if !style.on_pan.is_empty() {
            let mut gesture = Box::new(gesture::PanGesture::default());
            gesture.on_pan = style.take_on_pan();
            insert(gesture);
        }

        if !style.on_swipe.is_empty() {
            let mut gesture = Box::new(gesture::SwipeGesture::default());
            gesture.on_swipe = style.take_on_swipe();
            insert(gesture);
        }
    })
}

impl Deref for Node {
    type Target = NodeRef;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait IntoNode {
    fn into_node(self) -> Node;

    fn show_if(self, condition: impl Into<Prop<bool>>) -> Owned<Children>
    where
        Self: Sized,
    {
        children::show_if(condition.into(), self.into_node())
    }
}

impl IntoNode for Node {
    fn into_node(self) -> Node {
        self
    }
}

pub trait Component: IntoNode + Styleable + StyleableExt {}
impl<T: IntoNode + Styleable + StyleableExt> Component for T {}

#[cfg(test)]
mod component_tests {
    use std::{cell::Cell, panic::AssertUnwindSafe};

    use mue_core::{
        effect::{computed, on_cleanup, watch_effect},
        scope::{inject, provide},
    };

    use super::*;
    use crate::{
        factory::factory,
        layout::use_layout,
        theme::{provide_theme, theme},
    };

    #[derive(Clone, Default)]
    struct Probe {
        locals: Rc<RefCell<Vec<Signal<i32>>>>,
        models: Rc<RefCell<Vec<Signal<i32>>>>,
        widths: Rc<RefCell<Vec<ReadSignal<f32>>>>,
        runs: Rc<Cell<usize>>,
        cleanups: Rc<Cell<usize>>,
        contexts: Rc<RefCell<Vec<Option<&'static str>>>>,
        leaf_contexts: Rc<RefCell<Vec<Option<&'static str>>>>,
    }

    #[mue_macros::node]
    fn headless_leaf(style: &mut Style) {
        use_layout(style);
    }

    #[mue_macros::node]
    fn context_leaf(style: &mut Style, probe: Probe) {
        probe
            .get_clone()
            .leaf_contexts
            .borrow_mut()
            .push(inject::<&'static str>());
        use_layout(style);
    }

    #[mue_macros::component]
    fn stateful(
        probe: Probe,
        #[model]
        #[default(7)]
        model: i32,
    ) -> impl Component {
        let probe = probe.get_clone();
        let local = signal(3);
        let width = computed(move |_| (local.get() + model.get()) as f32);
        probe.locals.borrow_mut().push(local);
        probe.models.borrow_mut().push(model);
        probe.widths.borrow_mut().push(width);
        let runs = probe.runs.clone();
        let cleanups = probe.cleanups.clone();
        watch_effect(move || {
            local.get();
            model.get();
            runs.set(runs.get() + 1);
            let cleanups = cleanups.clone();
            on_cleanup(move || cleanups.set(cleanups.get() + 1));
        });
        headless_leaf().width(width)
    }

    #[mue_macros::component]
    fn contextual(probe: Probe, #[default(false)] override_context: bool) -> impl Component {
        let probe = probe.get_clone();
        probe.contexts.borrow_mut().push(inject::<&'static str>());
        if override_context.get() {
            provide("component");
        }
        context_leaf(Prop::Static(probe))
    }

    #[mue_macros::component]
    fn nested(probe: Probe) -> impl Component {
        provide("nested");
        contextual(probe).override_context(true)
    }

    fn width(node: &Node) -> taffy::Dimension {
        Runtime::with(|rt| {
            let layout = rt.node(node.id).layout_id.unwrap();
            rt.taffy.borrow().style(layout).unwrap().size.width
        })
    }

    #[test]
    fn local_signals_and_effects_belong_to_the_returned_node() {
        let parent = Scope::new().owned();
        let parent_signal = parent.run(|| signal(0));
        let probe = Probe::default();
        let node = parent.run(|| stateful(Prop::Static(probe.clone())).into_node());
        let local = probe.locals.borrow()[0];
        let computed = probe.widths.borrow()[0];
        assert_eq!(computed.get(), 10.);
        assert_eq!(probe.runs.get(), 1);
        local.set(5);
        assert_eq!(computed.get(), 12.);
        assert_eq!(probe.runs.get(), 2);
        drop(node);
        assert!(local.is_disposed());
        assert!(computed.is_disposed());
        assert_eq!(probe.cleanups.get(), 2);
        // The parent's lifetime deliberately extends past the component's.
        assert!(!parent_signal.is_disposed());
    }

    #[test]
    fn externally_supplied_model_outlives_the_component() {
        let owner = Scope::new().owned();
        let external = owner.run(|| signal(20));
        let probe = Probe::default();
        let node = owner.run(|| {
            stateful(Prop::Static(probe.clone()))
                .model(external)
                .into_node()
        });
        external.set(30);
        assert_eq!(probe.widths.borrow()[0].get(), 33.);
        let runs = probe.runs.get();
        drop(node);
        assert!(!external.is_disposed());
        external.set(40);
        assert_eq!(external.get(), 40);
        assert_eq!(probe.runs.get(), runs);
        drop(owner);
        assert!(external.is_disposed());
    }

    #[test]
    fn default_models_are_lazy_and_owned_by_each_component() {
        let owner = Scope::new().owned();
        let probe = Probe::default();
        let first = stateful(Prop::Static(probe.clone()));
        let second = stateful(Prop::Static(probe.clone()));
        assert!(probe.models.borrow().is_empty());
        let (first, second) = owner.run(|| (first.into_node(), second.into_node()));
        let first_model = probe.models.borrow()[0];
        let second_model = probe.models.borrow()[1];
        first_model.set(11);
        assert_eq!(second_model.get(), 7);
        drop(first);
        assert!(first_model.is_disposed());
        assert!(!second_model.is_disposed());
        drop(second);
        assert!(second_model.is_disposed());
    }

    #[test]
    fn sibling_component_contexts_are_isolated() {
        let owner = Scope::new().owned();
        let probe = Probe::default();
        let (first, second) = owner.run(|| {
            provide("parent");
            let first = contextual(Prop::Static(probe.clone()))
                .override_context(true)
                .into_node();
            assert_eq!(inject::<&'static str>(), Some("parent"));
            let second = contextual(Prop::Static(probe.clone())).into_node();
            (first, second)
        });
        assert_eq!(*probe.contexts.borrow(), [Some("parent"), Some("parent")]);
        assert_eq!(
            *probe.leaf_contexts.borrow(),
            [Some("component"), Some("parent")]
        );
        drop((first, second));
    }

    #[test]
    fn nested_components_retain_context_without_wrapper_nodes() {
        let owner = Scope::new().owned();
        let probe = Probe::default();
        let before = Runtime::with(|rt| rt.nodes.borrow().len());
        let node = owner.run(|| nested(Prop::Static(probe.clone())).into_node());
        assert_eq!(*probe.contexts.borrow(), [Some("nested")]);
        assert_eq!(*probe.leaf_contexts.borrow(), [Some("component")]);
        Runtime::with(|rt| {
            assert_eq!(rt.nodes.borrow().len(), before + 1);
            assert_eq!(rt.node(node.id).retained_scopes.len(), 2);
            assert!(rt.node(node.id).children.get_clone().is_empty());
        });
        drop(node);
        assert_eq!(Runtime::with(|rt| rt.nodes.borrow().len()), before);
    }

    #[mue_macros::component]
    fn styled(style: &mut Style) -> impl Component + use<> {
        // Keep the existing result-first style precedence and hook ordering.
        style.height = Some(42f32.into());
        let events = inject::<Rc<RefCell<Vec<&'static str>>>>().unwrap();
        headless_leaf().width(12.).on_render(move |_| {
            events.borrow_mut().push("result");
        })
    }

    #[test]
    fn component_style_precedence_and_hook_order_are_unchanged() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let owner = Scope::new().owned();
        owner.provide(events.clone());
        let default_events = events.clone();
        let node = owner.run(|| {
            styled()
                .width(99.)
                .height(1.)
                .on_render(move |_| {
                    default_events.borrow_mut().push("default");
                })
                .into_node()
        });
        assert_eq!(width(&node), taffy::Dimension::length(12.));
        Runtime::with(|rt| {
            let layout = rt.node(node.id).layout_id.unwrap();
            assert_eq!(
                rt.taffy.borrow().style(layout).unwrap().size.height,
                taffy::Dimension::length(42.)
            );
        });
        Runtime::with(|rt| rt.invoke_hook(node.id, |hooks| &mut hooks.render, &()));
        assert_eq!(*events.borrow(), ["default", "result"]);
    }

    #[test]
    fn factory_and_reactive_theme_work_with_component_scopes() {
        let owner = Scope::new().owned();
        let probe = Probe::default();
        let factory_probe = probe.clone();
        let factory = factory(move |()| contextual(Prop::Static(factory_probe.clone())))
            .style(|style, _| style.width(20.));
        let themed_factory = factory.clone();
        let initial = theme(move |component, style| {
            if component.options(&themed_factory).is_some() {
                style.width(30.)
            } else {
                style
            }
        });
        let themed_factory = factory.clone();
        let changed = theme(move |component, style| {
            if component.options(&themed_factory).is_some() {
                style.width(40.)
            } else {
                style
            }
        });
        let source = owner.run(|| signal(initial));
        let before = Runtime::with(|rt| rt.nodes.borrow().len());
        let node = owner.run(|| provide_theme(source, || factory.create(())).into_node());
        assert_eq!(width(&node), taffy::Dimension::length(30.));
        assert_eq!(Runtime::with(|rt| rt.nodes.borrow().len()), before + 1);
        source.set(changed);
        assert_eq!(width(&node), taffy::Dimension::length(40.));
        drop(node);
        assert!(!source.is_disposed());
        assert_eq!(Runtime::with(|rt| rt.nodes.borrow().len()), before);
    }

    #[test]
    fn helper_keeps_node_identity_and_releases_scope_on_failure() {
        let owner = Scope::new().owned();
        let original = Node::build(|| {});
        let id = original.id;
        let node = owner.run(|| Node::build_component(|| original));
        assert_eq!(node.id, id);
        drop(node);

        let local = Cell::new(None);
        let cleanups = Rc::new(Cell::new(0));
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            owner.run(|| {
                Node::build_component(|| {
                    local.set(Some(signal(1)));
                    let cleanups = cleanups.clone();
                    watch_effect(move || {
                        let cleanups = cleanups.clone();
                        on_cleanup(move || cleanups.set(cleanups.get() + 1));
                    });
                    panic!("component construction failed");
                })
            })
        }));
        assert!(result.is_err());
        assert!(local.get().unwrap().is_disposed());
        assert_eq!(cleanups.get(), 1);
    }

    #[test]
    fn child_cleanup_does_not_run_a_tearing_down_parent_watcher() {
        let owner = Scope::new().owned();
        let source = owner.run(|| signal(0));
        let child_cleanups = Rc::new(Cell::new(0));
        let cleanup_count = child_cleanups.clone();
        let child = Node::build(|| {
            watch_effect(move || {
                let cleanup_count = cleanup_count.clone();
                on_cleanup(move || {
                    cleanup_count.set(cleanup_count.get() + 1);
                    source.set(2);
                });
            });
        });
        let child_ref = *child;
        let parent_ref = Rc::new(Cell::new(None::<NodeRef>));
        let observed_parent = parent_ref.clone();
        let runs = Rc::new(Cell::new(0));
        let watcher_runs = runs.clone();
        let parent = Node::build_with_style(Style::new().children(child), |style| {
            use_layout(style);
            watch_effect(move || {
                source.get();
                watcher_runs.set(watcher_runs.get() + 1);
                if let Some(parent) = observed_parent.get() {
                    Runtime::with(|rt| {
                        // This access would panic if child cleanup triggered the
                        // watcher after its parent was removed from the arena.
                        let layout_id = rt.node(parent.id).layout_id.unwrap();
                        assert!(rt.taffy.borrow().layout(layout_id).is_ok());
                    });
                }
            });
        });
        parent_ref.set(Some(*parent));
        source.set(1);
        assert_eq!(runs.get(), 2);
        drop(parent);
        assert_eq!(child_cleanups.get(), 1);
        assert_eq!(source.get(), 2);
        assert_eq!(runs.get(), 2);
        Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            assert!(!nodes.contains_key(parent_ref.get().unwrap().id));
            assert!(!nodes.contains_key(child_ref.id));
        });
        source.set(3);
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn cleanup_panic_still_releases_node_scopes_and_measurement_context() {
        let owner = Scope::new().owned();
        let source = owner.run(|| signal(10.));
        let node_count = Runtime::with(|rt| rt.nodes.borrow().len());
        let layout_count = Runtime::with_taffy_mut(|taffy| taffy.total_node_count());
        let local_signals = RefCell::new(Vec::new());
        let cleanups = Rc::new(Cell::new(0));
        let mut layout = None;
        let parent = Node::build_component(|| {
            local_signals.borrow_mut().push(signal(1));
            let retained_cleanups = cleanups.clone();
            watch_effect(move || {
                let retained_cleanups = retained_cleanups.clone();
                on_cleanup(move || retained_cleanups.set(retained_cleanups.get() + 1));
            });
            let child = Node::build(|| {
                local_signals.borrow_mut().push(signal(2));
                watch_effect(|| {
                    on_cleanup(|| panic!("child cleanup failed"));
                });
            });
            Node::build_with_style(Style::new().children(child), |style| {
                local_signals.borrow_mut().push(signal(3));
                let node_cleanups = cleanups.clone();
                watch_effect(move || {
                    let node_cleanups = node_cleanups.clone();
                    on_cleanup(move || node_cleanups.set(node_cleanups.get() + 1));
                });
                layout = Some(use_layout(style));
            })
        });
        let layout = layout.unwrap();
        let context_token = Rc::new(());
        let context_alive = Rc::downgrade(&context_token);
        // Install outside all node scopes: only releasing the Taffy context
        // can drop this captured token and its owned measurement reaction.
        layout.set_measure_fn(move |_, _| {
            let _keep_alive = &context_token;
            taffy::Size {
                width: source.get(),
                height: 1.,
            }
        });
        Runtime::with_taffy_mut(|taffy| {
            taffy
                .compute_layout_with_measure(
                    layout.layout_id,
                    taffy::Size {
                        width: taffy::AvailableSpace::MaxContent,
                        height: taffy::AvailableSpace::MaxContent,
                    },
                    |known, available, _, context, _| {
                        context.map_or(taffy::Size::ZERO, |measure| {
                            measure.measure(known, available)
                        })
                    },
                )
                .unwrap();
        });
        assert!(context_alive.upgrade().is_some());
        let parent_ref = *parent;
        let error = std::panic::catch_unwind(AssertUnwindSafe(|| parent_ref.dispose()))
            .expect_err("cleanup panic must propagate after releasing resources");
        assert_eq!(error.downcast_ref::<&str>(), Some(&"child cleanup failed"));
        assert_eq!(cleanups.get(), 2);
        assert!(
            local_signals
                .borrow()
                .iter()
                .all(|signal| signal.is_disposed())
        );
        assert!(context_alive.upgrade().is_none());
        Runtime::with(|rt| assert_eq!(rt.nodes.borrow().len(), node_count));
        Runtime::with_taffy_mut(|taffy| {
            assert_eq!(taffy.total_node_count(), layout_count);
            assert!(taffy.get_node_context(layout.layout_id).is_none());
        });
        // Repeated explicit disposal and the owning handle's Drop are no-ops.
        assert!(
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                parent_ref.dispose();
                drop(parent);
            }))
            .is_ok()
        );
        assert_eq!(cleanups.get(), 2);
        source.set(11.);
        assert_eq!(source.get(), 11.);
    }
}
