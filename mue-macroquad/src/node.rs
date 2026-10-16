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
mod font_size_tests {
    use super::*;
    use crate::{
        layout::use_layout,
        style::FontSize,
        viewport::{self, Viewport},
    };
    use taffy::prelude::TaffyMaxContent;

    fn container(style: Style) -> Node {
        Node::build_with_style(style, |style| {
            use_layout(style);
        })
    }

    fn font(node: NodeRef) -> f32 {
        Runtime::with(|rt| rt.node(node.id).computed_font_size.get_untracked())
    }

    fn children(node: NodeRef) -> Rc<[NodeRef]> {
        Runtime::with(|rt| rt.node(node.id).children.get_clone())
    }

    #[test]
    fn prebuilt_children_inherit_computed_values_and_track_unit_changes() {
        let parent_size = signal(FontSize::Px(20.));
        let child_size = signal(FontSize::Percent(150.));
        let grandchild = container(Style::new());
        let grandchild_ref = *grandchild;
        let child = container(Style::new().font_size(child_size).children(grandchild));
        let child_ref = *child;
        let root = container(Style::new().font_size(parent_size).children(child));
        root.resolve_font_sizes();
        assert_eq!(font(*root), 20.);
        assert_eq!(font(child_ref), 30.);
        assert_eq!(font(grandchild_ref), 30.);
        parent_size.set(FontSize::Px(24.));
        root.resolve_font_sizes();
        assert_eq!(font(grandchild_ref), 36.);
        child_size.set(FontSize::Rem(2.));
        root.resolve_font_sizes();
        assert_eq!(font(child_ref), 48.);
        child_size.set(FontSize::Px(18.));
        root.resolve_font_sizes();
        assert_eq!(font(grandchild_ref), 18.);
        assert_eq!(child_size.get(), FontSize::Px(18.));
    }

    #[test]
    fn rem_uses_root_not_parent_and_root_relative_sizes_have_a_fixed_base() {
        let root_size = signal(FontSize::Percent(150.));
        let rem = container(Style::new().font_size(FontSize::Rem(2.)));
        let rem_ref = *rem;
        let parent = container(Style::new().font_size(10.).children(rem));
        let root = container(Style::new().font_size(root_size).children(parent));
        root.resolve_font_sizes();
        assert_eq!(font(*root), 48.);
        assert_eq!(font(rem_ref), 96.);
        root_size.set(FontSize::Rem(2.));
        root.resolve_font_sizes();
        assert_eq!(font(*root), 64.);
        assert_eq!(font(rem_ref), 128.);
    }

    #[test]
    fn viewport_font_declarations_are_preserved_and_resize_before_layout() {
        let declared = signal(FontSize::Vw(5.));
        let root = container(Style::new().font_size(declared));
        viewport::set(Viewport {
            width: 800.,
            height: 600.,
        });
        root.resolve_font_sizes();
        assert_eq!(font(*root), 40.);
        viewport::set(Viewport {
            width: 1000.,
            height: 800.,
        });
        root.resolve_font_sizes();
        assert_eq!(font(*root), 50.);
        assert_eq!(declared.get(), FontSize::Vw(5.));
        declared.set(FontSize::Vh(5.));
        root.resolve_font_sizes();
        assert_eq!(font(*root), 40.);
    }

    #[test]
    fn reactive_children_receive_font_context_when_attached_or_shown() {
        let shown = signal(false);
        let child = container(Style::new().font_size(FontSize::Em(2.)));
        let child_ref = *child;
        let root = container(Style::new().font_size(20.).children(child.show_if(shown)));
        root.resolve_font_sizes();
        assert!(children(*root).is_empty());
        shown.set(true);
        root.resolve_font_sizes();
        assert_eq!(font(child_ref), 40.);
        shown.set(false);
        root.resolve_font_sizes();
        shown.set(true);
        root.resolve_font_sizes();
        assert_eq!(font(child_ref), 40.);
    }

    #[test]
    fn font_subscribers_mount_children_before_the_same_layout() {
        let shown = signal(false);
        let child = container(Style::new().font_size(FontSize::Em(2.)));
        let child_ref = *child;
        let root = Node::build_with_style(
            Style::new().font_size(20.).children(child.show_if(shown)),
            |style| {
                let font_size = use_font_size();
                mue_core::effect::watch_effect(move || {
                    shown.set(font_size.get() == 20.);
                });
                use_layout(style);
            },
        );
        root.resolve_font_sizes();
        assert_eq!(children(*root).len(), 1);
        assert_eq!(font(child_ref), 40.);
    }

    #[test]
    fn theme_without_font_declaration_does_not_block_inheritance() {
        let theme = Signal::shallow(Rc::new(Style::new()));
        let defaults = Style::themed_defaults(*theme, Style::new());
        let child = container(defaults);
        let child_ref = *child;
        let root = container(Style::new().font_size(20.).children(child));
        root.resolve_font_sizes();
        assert_eq!(font(child_ref), 20.);
        theme.set(Rc::new(Style::new().font_size(FontSize::Percent(150.))));
        root.resolve_font_sizes();
        assert_eq!(font(child_ref), 30.);
        theme.set(Rc::new(Style::new()));
        root.resolve_font_sizes();
        assert_eq!(font(child_ref), 20.);
    }

    #[test]
    fn resolved_font_updates_actual_text_measurements_in_the_same_layout() {
        Runtime::with_fonts_mut(|fonts| {
            fonts.load_font_data(
                include_bytes!("../../examples/basics/assets/fonts/SpaceMono-Regular.ttf").to_vec(),
            );
        });
        let size = signal(FontSize::Px(20.));
        let label = text("MMMM")
            .font_family(cosmic_text::FamilyOwned::Name("Space Mono".into()))
            .into_node();
        let label_ref = *label;
        let root = container(Style::new().font_size(size).items_start().children(label));
        let measure_width = || {
            root.resolve_font_sizes();
            Runtime::with_taffy_mut(|taffy| {
                let (root_id, label_id) = Runtime::with(|rt| {
                    (
                        rt.node(root.id).layout_id.unwrap(),
                        rt.node(label_ref.id).layout_id.unwrap(),
                    )
                });
                taffy
                    .compute_layout_with_measure(
                        root_id,
                        taffy::Size::MAX_CONTENT,
                        |known, available, _, context, _| {
                            context.map_or(taffy::Size::ZERO, |f| f.measure(known, available))
                        },
                    )
                    .unwrap();
                taffy.layout(label_id).unwrap().size.width
            })
        };
        let first = measure_width();
        assert!(first > 0.);
        size.set(FontSize::Px(40.));
        let second = measure_width();
        assert!(second >= first * 1.9, "first={first}, second={second}");
        assert_eq!(font(label_ref), 40.);
    }
}
