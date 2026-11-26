//! Layout/geometry preparation is repeatable; frame updates run separately.

use super::{Children, NodeRef};
use crate::{
    math::{Rect, Vector},
    runtime::Runtime,
};
use mue_core::{batch, signal::Access};
use std::rc::Rc;
use taffy::{AvailableSpace, Size};

/// Geometry actually published during preparation, before any later root is run.
#[derive(Default)]
pub(crate) struct GeometrySnapshot {
    published: Vec<PublishedGeometry>,
    layout_roots: Vec<taffy::NodeId>,
}

struct PublishedGeometry {
    node: NodeRef,
    parent: Option<NodeRef>,
    rect: Option<Rect>,
    offset: Vector,
    children: Rc<[NodeRef]>,
}

fn child_list(children: Children) -> Rc<[NodeRef]> {
    if children.is_null() {
        Rc::from([])
    } else {
        children.get_clone_untracked()
    }
}

impl NodeRef {
    /// Compute layout and publish absolute logical-pixel bounds, clips and scroll
    /// geometry without drawing or advancing time. Call on a tree root; its
    /// origin is `(0, 0)` and available size is in logical pixels.
    ///
    /// Layout hooks and rect observers may update layout inputs; preparation
    /// repeats until Taffy and published geometry agree (up to 64 passes).
    /// Call outside an enclosing `mue_core::batch`, so each pass can flush rect
    /// observers. Repeated calls are safe within one frame. They publish the next geometry,
    /// so dispatch input for the displayed frame before calling this method.
    pub fn prepare_layout(&self, available_size: Vector) {
        self.prepare_layout_in(Rect::new(0., 0., available_size.x, available_size.y));
    }

    /// Prepare a tree within absolute logical-pixel bounds without drawing or
    /// advancing time. Taffy uses `bounds.size()` as its available constraints;
    /// published geometry starts at `bounds.min()`.
    ///
    /// Like [`Self::prepare_layout`], call outside an enclosing `mue_core::batch`
    /// so layout hooks and rect observers can converge before this returns.
    pub fn prepare_layout_in(&self, bounds: Rect) {
        self.prepare_layout_snapshot(bounds, &mut GeometrySnapshot::default());
    }

    pub(crate) fn prepare_layout_snapshot(
        &self,
        bounds: Rect,
        snapshot: &mut GeometrySnapshot,
    ) -> bool {
        let available_size = bounds.size();
        for _ in 0..64 {
            if !self.is_live() {
                return false;
            }
            self.resolve_font_sizes();
            let layout_roots = self.layout_roots();
            let mut changed = false;
            batch(|| {
                for &id in &layout_roots {
                    Runtime::with_taffy_mut(|taffy| {
                        taffy
                            .compute_layout_with_measure(
                                id,
                                Size {
                                    width: AvailableSpace::Definite(available_size.x),
                                    height: AvailableSpace::Definite(available_size.y),
                                },
                                |known, available, _, context, _| {
                                    context.map_or(Size::ZERO, |f| f.measure(known, available))
                                },
                            )
                            .unwrap();
                    });
                }
                changed = self.sync_layout();
            });
            if changed || !layout_is_clean(&self.layout_roots()) {
                continue;
            }

            snapshot.published.clear();
            let mut complete = false;
            batch(|| complete = self.publish_geometry(None, bounds.min(), &mut snapshot.published));
            if !self.is_live() {
                return false;
            }
            if !complete {
                continue;
            }
            self.resolve_font_sizes();
            if snapshot.published.iter().all(PublishedGeometry::is_current) {
                self.collect_layout_roots(&mut snapshot.layout_roots);
                if layout_is_clean(&snapshot.layout_roots) {
                    return true;
                }
            }
        }
        panic!("layout dependencies did not stabilize before rendering");
    }

    fn layout_roots(&self) -> Vec<taffy::NodeId> {
        let mut roots = Vec::new();
        self.collect_layout_roots(&mut roots);
        roots
    }

    fn collect_layout_roots(&self, roots: &mut Vec<taffy::NodeId>) {
        fn collect(node: NodeRef, roots: &mut Vec<taffy::NodeId>) {
            let state = Runtime::with(|rt| {
                let nodes = rt.nodes.borrow();
                let node = nodes.get(node.id)?;
                if let Some(id) = node.layout_id
                    && rt.taffy.borrow().parent(id).is_none()
                {
                    roots.push(id);
                }
                Some(*node.children)
            });
            if let Some(children) = state {
                for &child in child_list(children).iter() {
                    collect(child, roots);
                }
            }
        }
        roots.clear();
        collect(*self, roots);
    }

    fn layout_tree_is_dirty(&self) -> bool {
        Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let Some(id) = nodes.get(self.id).and_then(|node| node.layout_id) else {
                return false;
            };
            let taffy = rt.taffy.borrow();
            let mut root = id;
            while let Some(parent) = taffy.parent(root) {
                root = parent;
            }
            taffy.dirty(root).unwrap()
        })
    }

    fn publish_geometry(
        &self,
        parent: Option<NodeRef>,
        mut origin: Vector,
        published: &mut Vec<PublishedGeometry>,
    ) -> bool {
        // A layout hook can mount a new subtree. Never run its geometry hooks
        // against uncomputed zero sizes: return to measurement first.
        if self.layout_tree_is_dirty() {
            return false;
        }
        let geometry = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let node = nodes.get(self.id)?;
            node.layout_id.map(|id| {
                let taffy = rt.taffy.borrow();
                let layout = taffy.layout(id).unwrap();
                origin += Vector::new(layout.location.x, layout.location.y);
                let size = Vector::new(layout.size.width, layout.size.height);
                if node.center_on_layout_origin {
                    origin -= size / 2.;
                }
                (
                    node.layout.rect,
                    Rect::new(origin.x, origin.y, size.x, size.y),
                    (node.layout.border, crate::math::Edges::from(layout.border)),
                    (
                        node.layout.padding,
                        crate::math::Edges::from(layout.padding),
                    ),
                )
            })
        });
        if let Some((signal, rect, border, padding)) = geometry {
            signal.set(rect);
            for (signal, value) in [border, padding] {
                if !signal.is_null() && signal.get_untracked() != value {
                    signal.set(value);
                }
            }
        }
        Runtime::with(|rt| rt.invoke_hook(self.id, |hooks| &mut hooks.layout, &()));
        let state = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let node = nodes.get(self.id)?;
            Some((node.child_offset, *node.children))
        });
        let Some((offset, children)) = state else {
            return true;
        };
        let offset = offset.map_or(Vector::ZERO, |value| value.get_untracked());
        let children = child_list(children);
        if self.layout_tree_is_dirty() {
            return false;
        }
        published.push(PublishedGeometry {
            node: *self,
            parent,
            rect: geometry.map(|(_, rect, _, _)| rect),
            offset,
            children: children.clone(),
        });
        for child in children.iter() {
            if !child.publish_geometry(Some(*self), origin - offset, published) {
                return false;
            }
        }
        true
    }
}

fn layout_is_clean(roots: &[taffy::NodeId]) -> bool {
    Runtime::with(|rt| {
        let taffy = rt.taffy.borrow();
        roots
            .iter()
            .all(|&id| taffy.dirty(id).is_ok_and(|dirty| !dirty))
    })
}

impl GeometrySnapshot {
    pub(crate) fn nodes(&self) -> impl Iterator<Item = NodeRef> + '_ {
        self.published.iter().map(|geometry| geometry.node)
    }

    pub(crate) fn edges(&self) -> impl Iterator<Item = (NodeRef, Option<NodeRef>)> + '_ {
        self.published
            .iter()
            .map(|geometry| (geometry.node, geometry.parent))
    }

    /// Compare the published values before checking Taffy's dirtiness: reactive
    /// reads can run observers and dirty the tree.
    pub(crate) fn is_current(&self, root: NodeRef) -> bool {
        root.is_live()
            && self.published.iter().all(PublishedGeometry::is_current)
            && root.layout_roots() == self.layout_roots
            && layout_is_clean(&self.layout_roots)
    }
}

impl PublishedGeometry {
    fn is_current(&self) -> bool {
        let state = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let node = nodes.get(self.node.id)?;
            Some((node.layout.rect, node.child_offset, *node.children))
        });
        let Some((rect, offset, children)) = state else {
            return false;
        };
        self.rect
            .is_none_or(|value| rect.try_get_untracked() == Some(value))
            && offset.map_or(Vector::ZERO, |value| value.get_untracked()) == self.offset
            && child_list(children) == self.children
    }
}
