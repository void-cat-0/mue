mod children;
mod container;
mod geom;
pub mod image;
pub mod text;

pub use children::{Children, IntoChildren, join_children, map_keyed, show_if};
pub use container::{FlexboxBuilder, div, flexbox};
pub use geom::circle;
pub use image::image;
pub use text::text;

use std::{cell::RefCell, ops::Deref, rc::Rc};

use mue_core::{Disposable, Owned, prop::Prop, scope::Scope, signal::Access};
use slotmap::{Key, new_key_type};

use crate::{
    event::hit_test::HitTestFn,
    gesture::GestureId,
    hook::NodeHooks,
    layout::OwnedLayout,
    math::{Rect, Vector},
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
    pub hooks: NodeHooks,

    pub(crate) layout_id: Option<taffy::NodeId>,
    pub(crate) layout: OwnedLayout,
    pub(crate) hit_test: Option<HitTestFn>,
    pub(crate) gestures: Vec<GestureId>,

    pub(crate) children: Owned<Children>,
}

impl NodeInner {
    pub fn new(children: Owned<Children>) -> Self {
        Self {
            scope: Scope::new(),
            hooks: NodeHooks::default(),

            layout_id: None,
            layout: OwnedLayout::default(),
            hit_test: None,
            gestures: Vec::new(),

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
                node.layout
                    .rect
                    .set(Rect::new(origin.x, origin.y, size.width, size.height));
            }

            drop(node);
            rt.invoke_hook(self.id, |hooks| &mut hooks.render, &());

            let node = rt.node(self.id);
            if !node.children.is_null() {
                let children = *node.children;
                drop(node);
                for child in children.get_clone().iter() {
                    child.render(origin);
                }
            }
        });
    }
}

impl Disposable for NodeRef {
    fn dispose(&self) {
        let _ = Runtime::try_with(|rt| {
            let Some(node) = rt.nodes.borrow_mut().remove(self.id) else {
                return;
            };
            node.scope.dispose();
            if let Some(layout_id) = node.layout_id {
                rt.taffy.borrow_mut().remove(layout_id).unwrap();
            }
            if !node.children.is_null() {
                for child in node.children.get_clone().iter() {
                    child.dispose();
                }
                node.children.dispose();
            }
            for gesture_id in node.gestures {
                rt.gestures.borrow_mut().remove(gesture_id);
            }
        });
    }
}

#[repr(transparent)]
pub struct Node(Owned<NodeRef>);

impl Node {
    pub fn build(f: impl FnOnce()) -> Self {
        Self::build_with_style(Style::default(), |_style| f())
    }

    pub fn build_with_style(mut style: Style, f: impl FnOnce(&mut Style)) -> Self {
        let node = NodeInner::new(style.children.take().unwrap_or_else(|| ().into_children()));
        let scope = node.scope;
        CONTEXT.with_borrow_mut(|stack| stack.push(Box::new(node)));
        scope.run(|| f(&mut style));
        let mut node = CONTEXT.with_borrow_mut(|stack| stack.pop().unwrap());
        node.hooks = style.build_hooks();
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
