mod build;
pub(crate) use build::HoverOwner;
pub use build::{AppBuildContext, NodePlan, PrepareNode, PreparedNode, RootBuild, build};
pub mod atlas;
pub mod children;
pub mod flexbox;
pub mod shape;
pub mod text;
pub use atlas::GlyphAtlasStats;
pub use children::*;
pub use flexbox::{FlexboxBuilder, flexbox};
pub use shape::{ShapeBuilder, shape};
pub use text::{Span, SpanStyle, TextBuilder, TextOverflow, TextVerticalAlign, TextWrap, text};

use std::{cell::RefCell, ops::Deref, rc::Rc};

use mue_core::{
    Disposable, Owned,
    prop::Prop,
    scope::{OwnedScope, Scope},
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
    style::{
        FontSize, FontSizeExt, SizeExt, Style, Styleable, StyleableExt, Transform, TransformOrigin,
    },
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
    owned_scopes: Vec<OwnedScope>,
    hover_owners: Vec<Rc<HoverOwner>>,
    pub(crate) hovered: Option<Signal<bool>>,
    pub(crate) hovered_aliases: Vec<Signal<bool>>,
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
    pub(crate) matrix_transform: Prop<Matrix>,
    transform_list: Prop<Transform>,
    transform_origin: Prop<TransformOrigin>,
    pub(crate) font_size: Option<Prop<FontSize>>,
    pub(crate) computed_font_size: Signal<f32>,

    pub(crate) children: Owned<Children>,
}

impl NodeInner {
    pub(crate) fn effective_transform(&self) -> Matrix {
        let bounds = if self.layout.rect.is_null() {
            Rect::default()
        } else {
            self.layout.rect.get()
        };
        let transform_origin = self.transform_origin.get();
        let origin = bounds.min()
            + crate::math::Vector::new(
                match transform_origin.0 {
                    crate::style::Size::Percent(value) => bounds.w * value / 100.,
                    value => value.resolve("transform-origin"),
                },
                match transform_origin.1 {
                    crate::style::Size::Percent(value) => bounds.h * value / 100.,
                    value => value.resolve("transform-origin"),
                },
            );
        let around_origin = Matrix::from_translation(origin)
            * self.matrix_transform.get()
            * Matrix::from_translation(-origin);
        around_origin
            * self
                .transform_list
                .get_clone()
                .resolve(bounds, self.transform_origin.get())
    }

    pub fn new(scope: Scope, children: Owned<Children>) -> Self {
        Self {
            scope,
            retained_scopes: Vec::new(),
            owned_scopes: Vec::new(),
            hover_owners: Vec::new(),
            hovered: None,
            hovered_aliases: Vec::new(),
            hooks: NodeHooks::default(),

            layout_id: None,
            layout: OwnedLayout::default(),
            layout_sync: None,
            hit_test: None,
            gestures: Vec::new(),
            clip: None,
            child_offset: None,
            center_on_layout_origin: false,
            matrix_transform: Prop::Static(Matrix::IDENTITY),
            transform_list: Prop::default(),
            transform_origin: Prop::default(),
            font_size: None,
            computed_font_size: Signal::null(),

            children,
        }
    }

    pub fn with_mut<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        struct Restore(Option<Box<NodeInner>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                if let Some(node) = self.0.take() {
                    CONTEXT.with_borrow_mut(|stack| stack.push(node));
                }
            }
        }
        let mut restore =
            Restore(Some(CONTEXT.with_borrow_mut(|stack| {
                stack.pop().expect("no node context found")
            })));
        f(restore.0.as_mut().unwrap().as_mut())
    }

    #[cfg(test)]
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
    /// Observe an already enabled live node without allocating or retaining it.
    pub fn hovered(&self) -> Option<ReadSignal<bool>> {
        Runtime::try_with(|rt| {
            rt.nodes
                .borrow()
                .get(self.id)
                .and_then(|node| node.hovered.map(|signal| *signal))
        })
        .ok()
        .flatten()
    }
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

            let transform = node.effective_transform();
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

fn dispose_inner(node: NodeInner) {
    // Drop children and all owned reactive graphs before removing their layout storage.
    node.children.dispose();
    node.scope.dispose();
    for scope in node.retained_scopes {
        scope.dispose();
    }
    let _ = Runtime::try_with(|rt| {
        if let Some(layout_id) = node.layout_id {
            let _ = rt.taffy.borrow_mut().remove(layout_id);
        }
        for gesture_id in node.gestures {
            rt.gestures.borrow_mut().remove(gesture_id);
        }
    });
}
impl Disposable for NodeRef {
    fn dispose(&self) {
        let node = Runtime::try_with(|rt| rt.nodes.borrow_mut().remove(self.id))
            .ok()
            .flatten();
        if let Some(node) = node {
            dispose_inner(node);
        }
    }
}

#[repr(transparent)]
/// An owning node handle. Dropping it releases its children and reactive scope.
pub struct Node(Owned<NodeRef>);

impl Node {
    pub fn retain_scope(&self, scope: Scope) {
        Runtime::with(|rt| rt.nodes.borrow_mut()[self.id].retained_scopes.push(scope));
    }
    /// Build a custom node within its own reactive scope.
    pub fn build(f: impl FnOnce()) -> Self {
        Self::build_with_style(Style::default(), |_style| f())
    }

    pub fn retain_owned_scope(&self, scope: OwnedScope) {
        Runtime::with(|rt| rt.nodes.borrow_mut()[self.id].owned_scopes.push(scope));
    }

    /// Build a custom node. Its style is sealed before consumers execute.
    pub fn build_with_style(style: Style, f: impl FnOnce(&mut Style)) -> Self {
        let mut root = RootBuild::new();
        root.push_style(style);
        // This convenience entry has no declaration target or prepare defaults.
        let owner = root.into_untyped();
        Self::mount_parts(owner.0, owner.1, owner.2, owner.3, f)
    }

    pub(super) fn mount_prepared(prepared: PreparedNode) -> Self {
        let PreparedNode {
            owner,
            environment,
            style,
            hover_owners,
            mount,
        } = prepared;
        Self::mount_parts(owner, environment, style, hover_owners, mount)
    }

    fn mount_parts(
        owner: OwnedScope,
        environment: AppBuildContext,
        mut style: Style,
        hover_owners: Vec<Rc<HoverOwner>>,
        f: impl FnOnce(&mut Style),
    ) -> Self {
        struct MountGuard {
            depth: usize,
            armed: bool,
        }
        impl Drop for MountGuard {
            fn drop(&mut self) {
                if self.armed {
                    loop {
                        let node = CONTEXT.with_borrow_mut(|stack| {
                            if stack.len() > self.depth {
                                stack.pop()
                            } else {
                                None
                            }
                        });
                        match node {
                            Some(node) => dispose_inner(*node),
                            None => break,
                        }
                    }
                }
            }
        }
        let scope = owner.scope();
        environment.run(|| {
            scope.run(|| {
                let children = ().into_children();
                let mut node = NodeInner::new(scope, children);
                node.hovered = hover_owners.first().map(|owner| owner.signal);
                node.hovered_aliases = hover_owners
                    .iter()
                    .skip(1)
                    .map(|owner| owner.signal)
                    .collect();
                node.hover_owners = hover_owners;
                node.owned_scopes.push(owner);
                let depth = CONTEXT.with_borrow(|stack| stack.len());
                CONTEXT.with_borrow_mut(|stack| stack.push(Box::new(node)));
                let mut guard = MountGuard { depth, armed: true };
                crate::transition::bind_style(&mut style);
                crate::paint::bind_node_style(&mut style);
                // Lazy projection initialization can run user computations; do it
                // before borrowing the node build stack.
                let declared_font_size = style.declared_font_size();
                let matrix_transform = style.take_matrix_transform();
                let transform_list = style.take_transform_list();
                let transform_origin = style.take_transform_origin();
                NodeInner::with_mut(|node| {
                    node.computed_font_size = signal(FontSize::DEFAULT_PX);
                    node.font_size = declared_font_size;
                    node.matrix_transform = matrix_transform;
                    node.transform_list = transform_list;
                    node.transform_origin = transform_origin;
                });
                // Parent bindings exist before every independent child is materialized.
                if let Some(children) = style.children.take() {
                    let children = children.mount();
                    NodeInner::with_mut(|node| node.children = children);
                }
                f(&mut style);
                NodeInner::with_mut(|node| {
                    let after_children = std::mem::take(&mut node.hooks.after_children);
                    node.hooks = style.build_hooks();
                    node.hooks.after_children = after_children;
                    append_hooks(node, &mut style);
                });
                // Acquire the arena before removing the guarded node from its stack.
                let id = Runtime::with(|rt| {
                    let mut nodes = rt.nodes.borrow_mut();
                    let node = CONTEXT.with_borrow_mut(|stack| stack.pop().unwrap());
                    nodes.insert(*node)
                });
                guard.armed = false;
                Self(NodeRef { id }.owned())
            })
        })
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

        if !style.on_transform.is_empty() {
            let mut gesture = Box::new(gesture::TransformGesture::default());
            gesture.on_transform = style.take_on_transform();
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

    fn show_if(self, condition: impl Into<Prop<bool>>) -> ChildrenPlan
    where
        Self: Sized + 'static,
    {
        children::show_if(condition.into(), self)
    }
}

impl IntoNode for Node {
    fn into_node(self) -> Node {
        self
    }
}

pub trait Component: IntoNode + PrepareNode + Styleable + StyleableExt {}
impl<T: IntoNode + PrepareNode + Styleable + StyleableExt> Component for T {}

/// Take the current node's children so a component can wrap or reorder them.
pub fn take_children() -> Owned<Children> {
    NodeInner::with_mut(|node| std::mem::replace(&mut node.children, ().into_children()))
}

/// Set children while building a custom node, before calling `use_layout`.
pub fn set_children(children: Owned<Children>) {
    NodeInner::with_mut(|node| node.children = children);
}

/// Read the current node's bounds after its layout has been created.
pub fn use_rect() -> ReadSignal<Rect> {
    NodeInner::with_mut(|node| *node.layout.rect)
}

pub fn set_clip(rect: ReadSignal<Rect>) {
    NodeInner::with_mut(|node| node.clip = Some(rect));
}

pub fn set_child_offset(offset: ReadSignal<Vector>) {
    NodeInner::with_mut(|node| node.child_offset = Some(offset));
}

pub fn center_on_layout_origin() {
    NodeInner::with_mut(|node| node.center_on_layout_origin = true);
}

/// Return true when the callback changes layout inputs after receiving its size.
pub fn set_layout_sync(sync: impl Fn(Vector) -> bool + 'static) {
    NodeInner::with_mut(|node| node.layout_sync = Some(Rc::new(sync)));
}

pub fn after_children(hook: impl FnMut(&()) + 'static) {
    NodeInner::with_mut(|node| node.hooks.after_children.append(hook));
}

pub fn add_gesture(gesture: impl crate::gesture::Gesture + 'static) {
    let id = Runtime::with(|rt| rt.gestures.borrow_mut().insert(Box::new(gesture)));
    NodeInner::with_mut(|node| node.gestures.push(id));
}

impl NodeRef {
    pub fn layout_id(&self) -> Option<taffy::NodeId> {
        Runtime::with(|rt| rt.node(self.id).layout_id)
    }

    pub fn children(&self) -> Children {
        Runtime::with(|rt| *rt.node(self.id).children)
    }
}
