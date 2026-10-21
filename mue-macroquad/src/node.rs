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
            matrix_transform: Prop::Static(Matrix::IDENTITY),
            transform_list: Prop::default(),
            transform_origin: Prop::default(),
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

impl Disposable for NodeRef {
    fn dispose(&self) {
        let _ = Runtime::try_with(|rt| {
            let Some(node) = rt.nodes.borrow_mut().remove(self.id) else {
                return;
            };
            // The children signal owns the child Nodes through its callback.
            // Disposing it drops those Nodes, including hidden children.
            node.children.dispose();
            node.scope.dispose();
            for scope in node.retained_scopes {
                scope.dispose();
            }
            if let Some(layout_id) = node.layout_id {
                rt.taffy.borrow_mut().remove(layout_id).unwrap();
            }
            for gesture_id in node.gestures {
                rt.gestures.borrow_mut().remove(gesture_id);
            }
        });
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

    /// Build a custom node with layout, paint, and event styles.
    pub fn build_with_style(mut style: Style, f: impl FnOnce(&mut Style)) -> Self {
        let node = NodeInner::new(style.children.take().unwrap_or_else(|| ().into_children()));
        let scope = node.scope;
        CONTEXT.with_borrow_mut(|stack| stack.push(Box::new(node)));
        scope.run(|| {
            crate::transition::bind_style(&mut style);
            NodeInner::with_mut(|node| {
                node.computed_font_size = signal(FontSize::DEFAULT_PX);
                node.font_size = style.font_size;
            });
            // Apply once around the whole subtree, including custom draw hooks.
            NodeInner::with_mut(|node| {
                node.matrix_transform = style.take_matrix_transform();
                node.transform_list = style.take_transform_list();
                node.transform_origin = style.take_transform_origin();
            });
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

    fn factory_identity(&self) -> Option<usize> {
        None
    }

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
