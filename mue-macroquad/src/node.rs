pub(crate) mod build;
mod frame;
pub(crate) use build::HoverOwner;
pub use build::{BuildNode, NodeContext};
pub(crate) use frame::GeometrySnapshot;
pub mod atlas;
pub mod children;
pub mod flexbox;
pub mod portal;
pub mod shape;
pub mod text;
pub mod vector;
pub use atlas::GlyphAtlasStats;
pub use children::*;
pub use flexbox::{FlexboxBuilder, flexbox};
pub use portal::portal;
pub use shape::{ShapeBuilder, shape};
pub use text::{Span, SpanStyle, TextBuilder, TextOverflow, TextVerticalAlign, TextWrap, text};
pub use vector::{PathBuilder, SvgBuilder, path, svg};

use std::{cell::RefCell, marker::PhantomData, ops::Deref, rc::Rc};

use mue_core::{
    Disposable, Owned,
    prop::Prop,
    scope::{OwnedScope, Scope},
    signal::{Access, ReadSignal, Signal, signal},
};
use slotmap::{Key, new_key_type};

use crate::{
    clip::Sector,
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
    pub(crate) clip_radius: Option<Prop<f32>>,
    pub(crate) clip_sector: Option<Prop<Sector>>,
    pub(crate) child_offset: Option<ReadSignal<Vector>>,
    pub(crate) center_on_layout_origin: bool,
    pub(crate) matrix_transform: Prop<Matrix>,
    transform_list: Prop<Transform>,
    transform_origin: Prop<TransformOrigin>,
    pub(crate) font_size: Option<Prop<FontSize>>,
    pub(crate) computed_font_size: Signal<f32>,

    pub(crate) children: Owned<Children>,
}

pub(crate) struct NodeTransform {
    rect: ReadSignal<Rect>,
    matrix: Prop<Matrix>,
    list: Prop<Transform>,
    origin: Prop<TransformOrigin>,
}

impl NodeTransform {
    pub(crate) fn resolve(self) -> Matrix {
        let bounds = if self.rect.is_null() {
            Rect::default()
        } else {
            self.rect.get()
        };
        let transform_origin = self.origin.get();
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
            * self.matrix.get()
            * Matrix::from_translation(-origin);
        let list = match self.list {
            Prop::Static(list) => list,
            Prop::Dynamic(list) => list.get_clone(),
        };
        around_origin * list.resolve(bounds, transform_origin)
    }
}

impl NodeInner {
    pub(crate) fn transform(&self) -> NodeTransform {
        NodeTransform {
            rect: *self.layout.rect,
            matrix: self.matrix_transform,
            list: self.transform_list.clone(),
            origin: self.transform_origin,
        }
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
            clip_radius: None,
            clip_sector: None,
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
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
/// A borrowed node handle; copying it does not extend the node's lifetime.
/// Node handles are confined to the thread that owns their runtime.
pub struct NodeRef {
    pub(crate) id: NodeId,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for NodeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeRef({:?})", self.id.data())
    }
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
    pub(crate) fn from_id(id: NodeId) -> Self {
        Self {
            id,
            _thread_affinity: PhantomData,
        }
    }

    pub(crate) fn null() -> Self {
        Self::from_id(NodeId::null())
    }

    pub(crate) fn sync_layout(&self) -> bool {
        let state = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let node = nodes.get(self.id)?;
            let size = node.layout_sync.as_ref().and(node.layout_id).map(|id| {
                let size = rt.taffy.borrow().layout(id).unwrap().size;
                Vector::new(size.width, size.height)
            });
            Some((node.layout_sync.clone(), size))
        });
        let Some((sync, size)) = state else {
            return false;
        };
        let mut changed = match (sync, size) {
            (Some(sync), Some(size)) => sync(size),
            _ => false,
        };
        let children =
            Runtime::with(|rt| rt.nodes.borrow().get(self.id).map(|node| *node.children));
        if let Some(children) = children.and_then(|children| children.try_get_clone_untracked()) {
            for child in children.iter() {
                changed |= child.sync_layout();
            }
        }
        changed
    }

    /// Draw a prepared tree without computing layout or advancing frame state.
    ///
    /// Call [`Self::prepare_layout`] after geometry changes. The node owner must
    /// remain alive throughout this call. Custom render hooks must only draw or
    /// update rendering caches; changes to layout/state belong in update hooks.
    /// This queues macroquad draws and brackets the UI text-atlas pass, which
    /// can flush pending draws. It does not manage a host GPU pass or flush
    /// texture deletions; the host retains camera/pass ownership.
    pub fn render(&self) {
        Runtime::begin_text_render_pass();
        mue_core::batch(|| self.render_inner());
        Runtime::end_text_render_pass();
    }

    fn render_inner(&self) {
        Runtime::with(|rt| {
            let transform = rt.node(self.id).transform();
            let transform = transform.resolve();
            crate::paint::with_transform(transform, || {
                rt.invoke_hook(self.id, |hooks| &mut hooks.render, &());

                let (clip, clip_radius, clip_sector, layout_rect, children) = {
                    let node = rt.node(self.id);
                    (
                        node.clip,
                        node.clip_radius,
                        node.clip_sector,
                        *node.layout.rect,
                        *node.children,
                    )
                };
                let clip = clip.map(|rect| rect.get());
                let draw = || {
                    let draw_children = || {
                        if !children.is_null() {
                            for child in children.get_clone().iter() {
                                child.render_inner();
                            }
                        }
                        rt.invoke_hook(self.id, |hooks| &mut hooks.after_children, &());
                    };
                    if let Some(sector) = clip_sector {
                        let sector = sector.try_get().unwrap_or_default();
                        if sector.is_visible() {
                            crate::clip::with_clip_sector(layout_rect.get(), sector, draw_children);
                            return;
                        }
                    }
                    if let Some(radius) = clip_radius {
                        let radius = radius.try_get().unwrap_or(0.);
                        if radius > 0. {
                            crate::clip::with_clip_radius(layout_rect.get(), radius, draw_children);
                        } else {
                            draw_children();
                        }
                    } else {
                        draw_children();
                    }
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
            let state = Runtime::with(|rt| {
                let nodes = rt.nodes.borrow();
                let node = nodes.get(node.id)?;
                Some((node.font_size, node.computed_font_size, *node.children))
            });
            let Some((declaration, computed, children)) = state else {
                return false;
            };
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
        let measure = node.layout_id.and_then(|layout_id| {
            let mut taffy = rt.taffy_mut();
            let measure = crate::layout::take_measure(&mut taffy, layout_id);
            taffy.remove(layout_id).unwrap();
            measure
        });
        // Measure captures can own further Nodes.
        drop(measure);
        let removed: Vec<_> = {
            let mut gestures = rt.gestures.borrow_mut();
            node.gestures
                .into_iter()
                .filter_map(|id| gestures.remove(id))
                .collect()
        };
        // Gesture closures can own further Nodes. Drop outside the arena borrow.
        drop(removed);
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

/// An owning node handle. Dropping it releases its children and reactive scope.
#[repr(transparent)]
pub struct Node(Owned<NodeRef>);

impl Node {
    pub fn retain_scope(&self, scope: Scope) {
        Runtime::with(|rt| rt.nodes.borrow_mut()[self.id].retained_scopes.push(scope));
    }
    /// Create a leaf node without styles.
    pub fn empty() -> Self {
        NodeContext::new().finish_without_defaults(|_| {})
    }

    pub fn retain_owned_scope(&self, scope: OwnedScope) {
        Runtime::with(|rt| rt.nodes.borrow_mut()[self.id].owned_scopes.push(scope));
    }

    /// Create a leaf node with the given styles, configuring it in `body`.
    pub fn leaf(style: Style, body: impl FnOnce(&mut Style)) -> Self {
        let mut context = NodeContext::new();
        context.push_style(style);
        context.finish_without_defaults(body)
    }

    fn finish_parts(owner: OwnedScope, style: &mut Style, body: impl FnOnce(&mut Style)) -> Node {
        // A panic inside `body` unwinds out of the application: the runtime does not
        // attempt to recover, so the node stays on the CONTEXT stack and is not disposed.
        let scope = owner.scope();
        scope.run(|| {
            let children = ().into_children();
            let mut node = NodeInner::new(scope, children);
            node.owned_scopes.push(owner);
            CONTEXT.with_borrow_mut(|stack| stack.push(Box::new(node)));
            crate::transition::bind_style(style);
            crate::paint::bind_node_style(style);
            let font_size = style.declared_font_size();
            let matrix = style.take_matrix_transform();
            let transforms = style.take_transform_list();
            let origin = style.take_transform_origin();
            let hover_owners = std::mem::take(&mut style.hover_owners);
            NodeInner::with_mut(|node| {
                node.computed_font_size = signal(FontSize::DEFAULT_PX);
                node.font_size = font_size;
                node.matrix_transform = matrix;
                node.transform_list = transforms;
                node.transform_origin = origin;
                node.hovered = hover_owners.first().map(|owner| owner.signal);
                node.hovered_aliases = hover_owners
                    .iter()
                    .skip(1)
                    .map(|owner| owner.signal)
                    .collect();
                node.hover_owners = hover_owners;
            });
            if let Some(plan) = style.take_children_plan() {
                let children = plan.into_children();
                NodeInner::with_mut(|node| node.children = children);
            }
            body(style);
            let clip_radius = style.take_clip_radius();
            let clip_radius = crate::style::units::resolve_length(clip_radius, "clip-radius");
            if !matches!(clip_radius, Prop::Static(value) if value <= 0.) {
                NodeInner::with_mut(|node| node.clip_radius = Some(clip_radius));
            }
            let clip_sector = style.take_clip_sector();
            if !matches!(clip_sector, Prop::Static(sector) if !sector.is_visible()) {
                NodeInner::with_mut(|node| node.clip_sector = Some(clip_sector));
            }
            NodeInner::with_mut(|node| {
                let after_children = std::mem::take(&mut node.hooks.after_children);
                node.hooks = style.build_hooks();
                node.hooks.after_children = after_children;
                append_hooks(node, style);
            });
            // Acquire arena access before removing the node from its stack.
            let id = Runtime::with(|rt| {
                let mut nodes = rt.nodes.borrow_mut();
                let node = CONTEXT
                    .with_borrow_mut(|stack| stack.pop().expect("node being finished is missing"));
                nodes.insert(*node)
            });
            Self(NodeRef::from_id(id).owned())
        })
    }
}

fn append_hooks(node: &mut NodeInner, style: &mut Style) {
    use crate::gesture;

    Runtime::with(|rt| {
        let mut gestures = rt.gestures.borrow_mut();
        let mut insert = |gesture| node.gestures.push(gestures.insert(Some(gesture)));

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

/// Anything that produces one real node.
///
/// Implemented for [`Node`] itself and, through a blanket impl, for every
/// [`BuildNode`].
pub trait IntoNode {
    fn into_node(self) -> Node;

    fn show_if(self, condition: impl Into<Prop<bool>>) -> ChildrenPlan
    where
        Self: Sized + 'static,
    {
        children::show_if(condition.into(), self)
    }
}

// A prebuilt node owns the context it was built in, so it keeps its own honest
// identity impl. `Node` deliberately does not implement `BuildNode`: the
// blanket impl would then conflict with this one.
impl IntoNode for Node {
    fn into_node(self) -> Node {
        self
    }
}

pub trait Component: BuildNode + Styleable + StyleableExt {}
impl<T: BuildNode + Styleable + StyleableExt> Component for T {}

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

pub fn on_after_children(hook: impl FnMut(&()) + 'static) {
    NodeInner::with_mut(|node| node.hooks.after_children.append(hook));
}

pub fn add_gesture(gesture: impl crate::gesture::Gesture + 'static) {
    let id = Runtime::with(|rt| rt.gestures.borrow_mut().insert(Some(Box::new(gesture))));
    NodeInner::with_mut(|node| node.gestures.push(id));
}

impl NodeRef {
    /// Whether the node is still alive.
    pub fn is_live(&self) -> bool {
        Runtime::try_with(|rt| rt.nodes.borrow().contains_key(self.id)).unwrap_or(false)
    }

    /// The node's absolute logical-pixel bounds, published by layout preparation.
    /// `None` for disposed nodes and nodes without layout.
    pub fn rect(&self) -> Option<ReadSignal<Rect>> {
        Runtime::try_with(|rt| {
            rt.nodes
                .borrow()
                .get(self.id)
                .map(|node| *node.layout.rect)
                .filter(|rect| !rect.is_null())
        })
        .ok()
        .flatten()
    }

    /// The node's resolved font size in logical pixels, after layout preparation.
    pub fn font_size(&self) -> Option<f32> {
        Runtime::try_with(|rt| {
            rt.nodes
                .borrow()
                .get(self.id)
                .and_then(|node| node.computed_font_size.try_get_untracked())
        })
        .ok()
        .flatten()
    }

    pub fn layout_id(&self) -> Option<taffy::NodeId> {
        Runtime::with(|rt| rt.node(self.id).layout_id)
    }

    pub fn children(&self) -> Children {
        Runtime::with(|rt| *rt.node(self.id).children)
    }
}
