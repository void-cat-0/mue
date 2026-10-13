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
    signal::{Access, ReadSignal},
};
use slotmap::{Key, new_key_type};

use crate::{
    event::hit_test::HitTestFn,
    gesture::GestureId,
    hook::NodeHooks,
    layout::OwnedLayout,
    math::{Matrix, Rect, Vector},
    runtime::Runtime,
    style::{Style, Styleable, StyleableExt},
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
    pub(crate) hit_test: Option<HitTestFn>,
    pub(crate) gestures: Vec<GestureId>,
    pub(crate) clip: Option<ReadSignal<Rect>>,
    pub(crate) child_offset: Option<ReadSignal<Vector>>,
    pub(crate) center_on_layout_origin: bool,
    pub(crate) transform: Prop<Matrix>,

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
            hit_test: None,
            gestures: Vec::new(),
            clip: None,
            child_offset: None,
            center_on_layout_origin: false,
            transform: Prop::Static(Matrix::IDENTITY),

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
    pub(crate) fn retain_scope(&self, scope: Scope) {
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
            crate::animation::bind_style(&mut style);
            // Apply once around the whole subtree, including custom draw hooks.
            NodeInner::with_mut(|node| node.transform = style.take_transform());
            f(&mut style);
        });
        let mut node = CONTEXT.with_borrow_mut(|stack| stack.pop().unwrap());
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
