//! Overlay layers and teleports.
//!
//! A [`layer_host`] owns a fixed stack of [`Layer`]s above its content.
//! [`teleport`] builds a subtree where it is declared, so it keeps that
//! position's context (`provide`/`inject`, themes, inherited paint), but draws
//! and hit-tests it in the nearest host's layer, above all normal content. This
//! is Vue's `<Teleport>` and the basis for dialogs, menus, toasts, and
//! tooltips.
//!
//! [`App::new`](crate::App::new) wraps its root in a layer host. A custom
//! [`Scene`](crate::Scene) wraps each root that should accept teleports.
//! Without a host, a teleport draws its content in place.
//!
//! Each teleported subtree is placed in a full-size, pointer-transparent
//! surface over the host, so `position: absolute`, `center`, or
//! `anchored` (in `mue-components`) position the content inside it.
//! Surfaces are ordered by layer, then `z_index`, then insertion order.
//!
//! ```
//! use mue_macroquad::{overlay::layer::{Layer, teleport, layer_host}, node::{flexbox, IntoNode}, style::Styleable};
//!
//! let _app_root = layer_host(flexbox().children(
//!     teleport(Layer::Toast, || flexbox().width(200.).height(40.)),
//! ));
//! ```

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mue_core::{
    Disposable as _,
    effect::{computed, on_cleanup, watch_effect},
    prop::PropValue,
    scope::{inject, provide},
    signal::{Access, ReadSignal, Signal, signal},
};

use crate::{
    input::hit_test::{HitTestResult, hit_test_into, set_hit_test_fn},
    math::{Rect, Vector},
    node::{
        BuildNode, IntoChildren, IntoNode, Node, NodeContext, NodeRef, flexbox::use_container,
        set_children,
    },
    style::{Size, Style, Styleable, StyleableExt, values::Display},
};

/// Overlay layers, from lowest to highest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// Content pinned above scrolling content, such as sticky headers.
    Sticky,
    /// Dropdowns and selects attached to a trigger.
    Dropdown,
    /// Full-screen backdrops and drawers.
    Overlay,
    /// Dialogs.
    #[default]
    Modal,
    /// Popovers and menus, including those opened from a dialog.
    Popover,
    /// Toast notifications.
    Toast,
    /// Tooltips, always on top.
    Tooltip,
}
impl PropValue for Layer {}

#[derive(Clone, PartialEq)]
struct Entry {
    layer: Layer,
    z_index: i32,
    sequence: u64,
    surface: NodeRef,
}

/// Shared registry of one host's teleported surfaces.
#[derive(Clone)]
pub struct LayerHost {
    entries: Signal<Vec<Entry>>,
    sequence: Rc<Cell<u64>>,
    rect: Rc<Cell<Option<ReadSignal<Rect>>>>,
}

impl LayerHost {
    /// The nearest host provided by an ancestor [`layer_host`].
    pub fn current() -> Option<Self> {
        inject::<LayerHost>()
    }

    fn insert(&self, layer: Layer, z_index: i32, surface: NodeRef) -> u64 {
        let sequence = self.sequence.get();
        self.sequence.set(sequence + 1);
        self.entries.update(|entries| {
            entries.push(Entry {
                layer,
                z_index,
                sequence,
                surface,
            });
            entries.sort_by_key(|entry| (entry.layer, entry.z_index, entry.sequence));
        });
        sequence
    }

    fn remove(&self, sequence: u64) {
        if !self.entries.is_disposed() {
            self.entries.update(|entries| {
                entries.retain(|entry| entry.sequence != sequence);
            });
        }
    }

    /// The host's published bounds, once it has been laid out.
    pub fn rect(&self) -> Option<Rect> {
        self.rect.get()?.try_get_untracked()
    }

    /// Number of teleported surfaces currently in this host.
    pub fn len(&self) -> usize {
        self.entries
            .try_get_clone_untracked()
            .map_or(0, |entries| entries.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Builder returned by [`layer_host`].
pub struct LayerHostBuilder<N> {
    content: N,
    style: Style,
}

/// Host overlay layers above `content`. The host is a full-size block
/// container: it fills the space it is given and lays `content` out normally.
pub fn layer_host<N: IntoNode + 'static>(content: N) -> LayerHostBuilder<N> {
    LayerHostBuilder {
        content,
        style: Style::new(),
    }
}

impl<N> Styleable for LayerHostBuilder<N> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl<N: IntoNode + 'static> BuildNode for LayerHostBuilder<N> {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self { content, style } = self;
        let host = LayerHost {
            entries: signal(Vec::new()),
            sequence: Rc::new(Cell::new(0)),
            rect: Rc::new(Cell::new(None)),
        };
        // Provide before any child is built so teleports below can find it.
        provide(host.clone());
        ctx.push_style(style);
        let defaults = Style::new()
            .display(Display::Block)
            .relative()
            .width(Size::Percent(100.))
            .height(Size::Percent(100.));
        ctx.finish(defaults, move |style| {
            let content = content.into_node();
            let content_ref = *content;
            let entries = host.entries;
            let children = computed(move |_| {
                let _ = &content;
                let entries = entries.get_clone();
                let mut refs = Vec::with_capacity(entries.len() + 1);
                refs.push(content_ref);
                refs.extend(entries.iter().map(|entry| entry.surface));
                Rc::<[NodeRef]>::from(refs)
            })
            .owned();
            set_children(children);
            use_container(style);
            host.rect.set(Some(crate::node::use_rect()));
            // Render offscreen while any backdrop-blurred node needs a source.
            crate::node::NodeInner::with_mut(|node| {
                node.offscreen = Some(crate::paint::offscreen::OffscreenState::backdrop_root());
                // The wrapped content stays the root for `rem` units.
                node.font_root_transparent = true;
            });
        })
    }
}

/// Builder returned by [`teleport`].
pub struct TeleportBuilder<F> {
    layer: Layer,
    view: F,
    z_index: i32,
    style: Style,
}

/// Build `view` here but draw it in the nearest host's `layer`.
///
/// The returned node is an empty placeholder that takes no space. Dropping it
/// removes the teleported subtree. Styles set on the builder apply to the
/// full-size surface that holds the content in the layer.
pub fn teleport<F, N>(layer: Layer, view: F) -> TeleportBuilder<F>
where
    F: FnOnce() -> N + 'static,
    N: IntoNode,
{
    TeleportBuilder {
        layer,
        view,
        z_index: 0,
        style: Style::new(),
    }
}

impl<F> TeleportBuilder<F> {
    /// Order among surfaces in the same layer; higher draws on top.
    pub fn layer_z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }
}

impl<F> Styleable for TeleportBuilder<F> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

fn surface_defaults() -> Style {
    Style::new().absolute().inset((Size::Px(0.),))
}

/// Make the current node pointer-transparent: only its children can be hit.
fn pass_through() {
    set_hit_test_fn(Rc::new(
        |node: NodeRef, pos: Vector, result: &mut HitTestResult| {
            for &child in crate::node::paint_order(node.children().get_clone_untracked())
                .iter()
                .rev()
            {
                if hit_test_into(child, pos, result) {
                    return;
                }
            }
        },
    ));
}

/// A full-size, pointer-transparent surface holding `content`.
fn surface(mut style: Style, content: Node) -> Node {
    style.provide_defaults(surface_defaults());
    Node::leaf(style, move |style| {
        set_children(content.into_children());
        pass_through();
        use_container(style);
    })
}

impl<F, N> BuildNode for TeleportBuilder<F>
where
    F: FnOnce() -> N + 'static,
    N: IntoNode,
{
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            layer,
            view,
            z_index,
            style,
        } = self;
        let Some(host) = LayerHost::current() else {
            // No host: the placeholder itself becomes the surface, in place.
            ctx.push_style(style);
            return ctx.finish(surface_defaults(), move |style| {
                set_children(view().into_node().into_children());
                pass_through();
                use_container(style);
            });
        };
        ctx.finish(
            Style::new().absolute().width(0.).height(0.),
            move |placeholder| {
                crate::layout::use_layout(placeholder);
                let surface = surface(style, view().into_node());
                let sequence = host.insert(layer, z_index, *surface);
                let owned = Rc::new(RefCell::new(Some(surface)));
                // Remove the surface when this placeholder is disposed. The effect
                // has no dependencies, so it runs once; its cleanup runs on disposal.
                watch_effect(move || {
                    let owned = owned.clone();
                    let host = host.clone();
                    on_cleanup(move || {
                        host.remove(sequence);
                        drop(owned.borrow_mut().take());
                    });
                });
            },
        )
    }
}
