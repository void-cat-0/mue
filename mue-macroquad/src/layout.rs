use mue_core::prelude::*;
use taffy::{AvailableSpace, Size};

use crate::{math::Rect, node::NodeInner, runtime::Runtime, style::Style};

/// Compute intrinsic content size within Taffy's known dimensions and constraints.
pub trait MeasureFn {
    fn measure(
        &mut self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32>;
}

impl<F> MeasureFn for F
where
    F: FnMut(Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
{
    fn measure(
        &mut self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        self(known_dimensions, available_space)
    }
}

#[non_exhaustive]
/// A node's layout handle and current absolute bounds.
pub struct Layout {
    pub layout_id: taffy::NodeId,
    pub rect: ReadSignal<Rect>,
}

impl Layout {
    /// Supply intrinsic sizing. Reactive reads invalidate the cached measurement.
    pub fn set_measure_fn(&self, mut measure_fn: impl MeasureFn + 'static) {
        let reaction = create_reaction({
            let layout_id = self.layout_id;
            move || {
                Runtime::with_taffy_mut(|taffy| {
                    taffy
                        .mark_dirty(layout_id)
                        .expect("failed to mark layout dirty");
                });
            }
        })
        .owned();
        Runtime::with_taffy_mut(move |taffy| {
            taffy
                .set_node_context(
                    self.layout_id,
                    Some(Box::new(move |known_dimensions, available_space| {
                        reaction.track(|| measure_fn.measure(known_dimensions, available_space))
                    })),
                )
                .unwrap();
        });
    }

    /// Invalidate cached layout after non-reactive content changes.
    pub fn mark_dirty(&self) {
        Runtime::with_taffy_mut(|taffy| {
            taffy.mark_dirty(self.layout_id).unwrap();
        });
    }
}

pub(crate) struct OwnedLayout {
    pub rect: Signal<Rect>,
}

impl Default for OwnedLayout {
    fn default() -> Self {
        Self {
            rect: Signal::null(),
        }
    }
}

/// Create layout for the node being built. Call at most once per node.
pub fn use_layout(style: &mut Style) -> Layout {
    let style = style.build_taffy();
    NodeInner::with_mut(|node| {
        if node.layout_id.is_some() {
            panic!("use_layout can only be called once per node");
        }

        let layout_id =
            Runtime::with_taffy_mut(|taffy| taffy.new_leaf(Default::default()).unwrap());
        watch_effect(move || {
            Runtime::with_taffy_mut(|taffy| {
                taffy.set_style(layout_id, style.get_clone()).unwrap();
            });
        });

        node.layout_id = Some(layout_id);
        node.layout = OwnedLayout {
            rect: signal(Rect::default()),
        };

        let children = *node.children;
        node.scope.run(|| {
            watch_effect(move || {
                Runtime::with(|rt| {
                    let children = children.get_clone();
                    let mut taffy = rt.taffy.borrow_mut();
                    taffy.remove_children_range(layout_id, ..).unwrap();
                    for child_layout_id in children
                        .iter()
                        .filter_map(|node| rt.node(node.id).layout_id)
                    {
                        taffy.add_child(layout_id, child_layout_id).unwrap();
                    }
                });
            });
        });

        Layout {
            layout_id,
            rect: *node.layout.rect,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    fn headless_node() -> (Node, Layout) {
        let mut layout = None;
        let node = Node::build_with_style(Style::default(), |style| {
            layout = Some(use_layout(style));
        });
        (node, layout.unwrap())
    }

    fn compute_layout(layout: &Layout) {
        Runtime::with_taffy_mut(|taffy| {
            taffy
                .compute_layout_with_measure(
                    layout.layout_id,
                    Size {
                        width: AvailableSpace::MaxContent,
                        height: AvailableSpace::MaxContent,
                    },
                    |known, available, _, context, _| {
                        context.map_or(Size::ZERO, |measure| measure.measure(known, available))
                    },
                )
                .unwrap();
        });
        assert!(!is_dirty(layout));
    }

    fn is_dirty(layout: &Layout) -> bool {
        Runtime::with_taffy_mut(|taffy| taffy.dirty(layout.layout_id).unwrap())
    }

    #[test]
    fn replacing_measure_fn_unsubscribes_the_old_reaction() {
        let owner = Scope::new().owned();
        let (old_width, new_width) = owner.run(|| (signal(10.), signal(20.)));
        let (node, layout) = headless_node();
        // Install outside the Node's scope so closure ownership, rather than
        // eventual scope disposal, must release the replaced reaction.
        layout.set_measure_fn(move |_, _| Size {
            width: old_width.get(),
            height: 1.,
        });
        compute_layout(&layout);
        old_width.set(11.);
        assert!(is_dirty(&layout));
        compute_layout(&layout);

        layout.set_measure_fn(move |_, _| Size {
            width: new_width.get(),
            height: 1.,
        });
        compute_layout(&layout);
        old_width.set(12.);
        assert!(!is_dirty(&layout));
        new_width.set(21.);
        assert!(is_dirty(&layout));
        compute_layout(&layout);
        Runtime::with_taffy_mut(|taffy| {
            assert_eq!(taffy.layout(layout.layout_id).unwrap().size.width, 21.);
        });
        drop(node);
    }

    #[test]
    fn dropping_node_unsubscribes_its_measure_reaction() {
        let owner = Scope::new().owned();
        let width = owner.run(|| signal(10.));
        let (node, layout) = headless_node();
        // This reaction is not registered with the Node's scope: removing the
        // Taffy measurement closure must dispose it even in this case.
        layout.set_measure_fn(move |_, _| Size {
            width: width.get(),
            height: 1.,
        });
        compute_layout(&layout);
        width.set(11.);
        assert!(is_dirty(&layout));
        // Re-track after invalidation so a live subscription exists at drop.
        compute_layout(&layout);
        let count = Runtime::with_taffy_mut(|taffy| taffy.total_node_count());
        drop(node);
        Runtime::with_taffy_mut(|taffy| {
            assert_eq!(taffy.total_node_count(), count - 1);
        });
        assert!(!width.is_disposed());
        // A leaked reaction would call mark_dirty on the removed Taffy node
        // and panic here. The external signal must remain usable instead.
        width.set(12.);
        assert_eq!(width.get(), 12.);
    }
}
