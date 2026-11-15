//! Layout/geometry preparation is repeatable; frame updates run separately.
#[cfg(test)]
mod mutation_tests;
#[cfg(test)]
mod scene_tests;
#[cfg(test)]
mod tests;

use super::{Children, NodeRef};
use crate::{
    math::{Rect, Vector},
    runtime::Runtime,
};
use mue_core::{batch, signal::Access};
use std::rc::Rc;
use taffy::{AvailableSpace, Size};

/// Read-only evidence that a prepared tree still has its published geometry.
/// Empty snapshots are current; callers validate root liveness separately.
#[derive(Default)]
pub(crate) struct GeometrySnapshot {
    root: Option<NodeRef>,
    published: Vec<PublishedGeometry>,
    layout_roots: Vec<(taffy::NodeId, bool)>,
}

struct PublishedGeometry {
    node: NodeRef,
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
        let available_size = bounds.size();
        for _ in 0..64 {
            if !self.is_live() {
                return;
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
            let dirty = || {
                Runtime::with(|rt| {
                    self.layout_roots()
                        .iter()
                        .any(|&id| rt.taffy.borrow().dirty(id).unwrap())
                })
            };
            if changed || dirty() {
                continue;
            }

            let mut published = Vec::new();
            let mut complete = false;
            batch(|| complete = self.publish_geometry(bounds.min(), &mut published));
            if !self.is_live() {
                return;
            }
            if !complete {
                continue;
            }
            // Rect observers run when the batch flushes. They can change Taffy
            // inputs, child lists or offsets; offsets alone do not dirty Taffy.
            self.resolve_font_sizes();
            if published.iter().all(PublishedGeometry::is_current) && !dirty() {
                return;
            }
        }
        panic!("layout dependencies did not stabilize before rendering");
    }

    fn is_live(&self) -> bool {
        Runtime::with(|rt| rt.nodes.borrow().contains_key(self.id))
    }

    fn layout_roots(&self) -> Vec<taffy::NodeId> {
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
        let mut roots = Vec::new();
        collect(*self, &mut roots);
        roots
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

    fn publish_geometry(&self, mut origin: Vector, published: &mut Vec<PublishedGeometry>) -> bool {
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
                )
            })
        });
        if let Some((signal, rect)) = geometry {
            signal.set(rect);
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
            rect: geometry.map(|(_, rect)| rect),
            offset,
            children: children.clone(),
        });
        for child in children.iter() {
            if !child.publish_geometry(origin - offset, published) {
                return false;
            }
        }
        true
    }

    /// Freeze the live tree in parent-first order without invoking frame hooks.
    /// Keep repeated handles so App can reject overlapping scene trees.
    pub(crate) fn frame_nodes(&self) -> Vec<NodeRef> {
        fn collect(node: NodeRef, nodes: &mut Vec<NodeRef>) {
            let children =
                Runtime::with(|rt| rt.nodes.borrow().get(node.id).map(|node| *node.children));
            let Some(children) = children else {
                return;
            };
            let children = child_list(children);
            if !node.is_live() {
                return;
            }
            nodes.push(node);
            for &child in children.iter() {
                collect(child, nodes);
            }
        }
        let mut nodes = Vec::new();
        collect(*self, &mut nodes);
        nodes.retain(NodeRef::is_live);
        nodes
    }

    /// Capture published rects, offsets, child lists and Taffy dirtiness without
    /// computing layout or invoking layout hooks. A dead root has an empty snapshot.
    pub(crate) fn prepared_geometry(&self) -> GeometrySnapshot {
        if !self.is_live() {
            return GeometrySnapshot::default();
        }
        let published = self
            .frame_nodes()
            .into_iter()
            .filter_map(PublishedGeometry::capture)
            .collect();
        if !self.is_live() {
            return GeometrySnapshot::default();
        }
        let layout_roots = self
            .layout_roots()
            .into_iter()
            .map(|id| (id, Runtime::with(|rt| rt.taffy.borrow().dirty(id).unwrap())))
            .collect();
        GeometrySnapshot {
            root: Some(*self),
            published,
            layout_roots,
        }
    }

    /// App owns this once-per-frame call; layout and drawing never invoke it.
    /// Freeze the active node list before callbacks: disposed nodes are skipped,
    /// and nodes mounted during updates start updating on the next frame.
    #[cfg(test)]
    pub(crate) fn advance_frame(&self, dt: f32) {
        let nodes = self.frame_nodes();
        batch(|| {
            Runtime::with(|rt| {
                for node in nodes {
                    rt.invoke_hook(node.id, |hooks| &mut hooks.update, &dt);
                }
            })
        });
    }
}

impl GeometrySnapshot {
    /// Whether this tree is still clean and matches the captured publication.
    pub(crate) fn is_current(&self) -> bool {
        let Some(root) = self.root else {
            return true;
        };
        root.is_live()
            && self.published.iter().all(PublishedGeometry::is_current)
            && root.layout_roots()
                == self
                    .layout_roots
                    .iter()
                    .map(|&(id, _)| id)
                    .collect::<Vec<_>>()
            && Runtime::with(|rt| {
                let taffy = rt.taffy.borrow();
                self.layout_roots
                    .iter()
                    .all(|&(id, dirty)| !dirty && taffy.dirty(id).is_ok_and(|dirty| !dirty))
            })
    }
}

impl PublishedGeometry {
    fn capture(node: NodeRef) -> Option<Self> {
        let (rect, offset, children) = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let node = nodes.get(node.id)?;
            Some((node.layout.rect, node.child_offset, *node.children))
        })?;
        Some(Self {
            node,
            rect: rect.try_get_untracked(),
            offset: offset.map_or(Vector::ZERO, |value| value.get_untracked()),
            children: child_list(children),
        })
    }

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
