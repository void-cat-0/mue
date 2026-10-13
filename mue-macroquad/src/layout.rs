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
        });
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
