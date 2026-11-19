use std::mem;

use mue_core::prelude::*;
use taffy::{AvailableSpace, Size, TaffyTree};

use crate::{math::Rect, node::NodeInner, runtime::Runtime, style::Style};

/// Compute intrinsic content size within Taffy's known dimensions and constraints.
///
/// Runs synchronously inside the layout solver. Read sizing inputs, but do not
/// mutate reactive or layout state, drive layout, or call [`with_taffy`]. To observe
/// geometry, capture [`Layout::rect`] and read its last published value instead
/// (e.g. `rect.get_untracked()`). It does not reflect the in-progress solve.
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
/// A node's layout handle and absolute logical-pixel bounds, published by
/// [`crate::node::NodeRef::prepare_layout`] before drawing.
pub struct Layout {
    pub layout_id: taffy::NodeId,
    pub rect: ReadSignal<Rect>,
    scope: Scope,
}

impl Layout {
    /// Supply or replace the node's intrinsic sizing callback.
    /// Tracked reads invalidate cached measurement; the callback and its
    /// subscriptions belong to the node, not the scope that registers them.
    pub fn set_measure_fn(&self, measure_fn: impl MeasureFn + 'static) {
        let measure: Box<dyn MeasureFn> = Box::new(MeasureRegistration::new(self, measure_fn));
        let previous = Runtime::with_taffy_mut(move |taffy| {
            if let Some(current) = taffy.get_node_context_mut(self.layout_id) {
                let previous = mem::replace(current, measure);
                taffy.mark_dirty(self.layout_id).unwrap();
                Some(previous)
            } else {
                taffy
                    .set_node_context(self.layout_id, Some(measure))
                    .unwrap();
                None
            }
        });
        // Unsubscribe and drop user captures after releasing the solver borrow.
        drop(previous);
    }

    /// Invalidate cached layout after non-reactive content changes.
    pub fn mark_dirty(&self) {
        Runtime::with_taffy_mut(|taffy| {
            taffy.mark_dirty(self.layout_id).unwrap();
        });
    }
}

struct MeasureRegistration<F> {
    // Release the reaction before dropping the callback's captures.
    _scope: OwnedScope,
    reaction: Reaction,
    measure_fn: F,
}

impl<F: MeasureFn> MeasureRegistration<F> {
    fn new(layout: &Layout, measure_fn: F) -> Self {
        let scope = layout.scope.run(OwnedScope::new);
        layout.scope.push_subscope(scope.scope());
        let layout_id = layout.layout_id;
        let reaction = scope.run(|| {
            create_reaction(move || {
                Runtime::with_taffy_mut(|taffy| taffy.mark_dirty(layout_id).unwrap());
            })
        });
        Self {
            _scope: scope,
            reaction,
            measure_fn,
        }
    }
}

impl<F: MeasureFn> MeasureFn for MeasureRegistration<F> {
    fn measure(&mut self, known: Size<Option<f32>>, available: Size<AvailableSpace>) -> Size<f32> {
        self.reaction
            .track(|| self.measure_fn.measure(known, available))
    }
}

/// Taffy 0.9.2 removes the node without clearing its context. Move the user
/// callback out before clearing the entry so it can run Drop outside the borrow.
pub(crate) fn take_measure(
    taffy: &mut TaffyTree<Box<dyn MeasureFn>>,
    id: taffy::NodeId,
) -> Option<Box<dyn MeasureFn>> {
    let value = mem::replace(taffy.get_node_context_mut(id)?, Box::new(|_, _| Size::ZERO));
    taffy.set_node_context(id, None).unwrap();
    Some(value)
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
    bind_layout(style.build_taffy())
}

fn bind_layout(style: ReadSignal<taffy::Style>) -> Layout {
    NodeInner::with_mut(|node| {
        if node.layout_id.is_some() {
            panic!("use_layout can only be called once per node");
        }

        let layout_id =
            Runtime::with_taffy_mut(|taffy| taffy.new_leaf(Default::default()).unwrap());
        node.layout_id = Some(layout_id);
        watch_effect(move || {
            let style = style.get_clone();
            Runtime::with_taffy_mut(|taffy| {
                taffy.set_style(layout_id, style).unwrap();
            });
        });

        node.layout = OwnedLayout {
            rect: signal(Rect::default()),
        };

        let children = *node.children;
        node.scope.run(|| {
            watch_effect(move || {
                Runtime::with(|rt| {
                    let children = children.get_clone();
                    let mut taffy = rt.taffy_mut();
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
            scope: node.scope,
        }
    })
}

/// Inspect computed layout from a component without exposing the host runtime.
///
/// Do not call this from a [`MeasureFn`]. Use its sizing inputs or the last
/// published [`Layout::rect`] instead.
pub fn with_taffy<R>(f: impl FnOnce(&taffy::TaffyTree<Box<dyn MeasureFn>>) -> R) -> R {
    Runtime::with(|rt| {
        let taffy = rt.taffy.try_borrow().expect(
            "with_taffy cannot read a mutably borrowed Taffy tree; \
             in MeasureFn, use its inputs or the already-published Layout::rect",
        );
        f(&taffy)
    })
}
