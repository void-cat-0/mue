//! Animated keyed lists (Vue's `<TransitionGroup>`).
//!
//! [`transition_group`] maps a reactive list to nodes like
//! [`map_keyed`](crate::node::map_keyed), and animates the changes:
//!
//! - **enter**: a new item fades/scales/slides in with its
//!   [`PresenceAnimation`];
//! - **leave**: a removed item stays where it was (taken out of the flow)
//!   while it animates out, then it is disposed;
//! - **move**: items whose position changed (because of insertions,
//!   removals, or reordering) slide from their old place to the new one,
//!   using the FLIP technique (a transform that starts at the old position
//!   and eases to zero), so layout is computed once.
//!
//! Items present when the group is built appear without animation, as in Vue
//! without `appear`.
//!
//! ```
//! use mue_core::prelude::*;
//! use mue_macroquad::{
//!     node::{IntoNode, text},
//!     presence::PresenceAnimation,
//!     style::StyleableExt,
//!     transition_group::transition_group,
//! };
//! use std::rc::Rc;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let todos: Signal<Rc<[(u32, Rc<str>)]>> = signal(Rc::from([(1, Rc::from("Write docs"))]));
//! let _list = transition_group(todos, |todo| todo.0, |todo| text(todo.1.clone()))
//!     .animation(PresenceAnimation::fade().slide(0., -8.))
//!     .flex_column();
//! # });
//! ```

use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

use mue_core::{
    Disposable as _,
    effect::{computed, untracked, watch_effect},
    prop::Prop,
    scope::{OwnedScope, current_scope},
    signal::{Access, Signal, signal},
};

use crate::{
    math::{Rect, Vector},
    node::{
        BuildNode, IntoChildren, IntoNode, Node, NodeContext, NodeHandle, NodeRef, flexbox,
        flexbox::use_container, set_children,
    },
    presence::PresenceAnimation,
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp, values::Position},
    transition::Transition,
};

/// Builder returned by [`transition_group`].
pub struct TransitionGroupBuilder<T: 'static, K: 'static> {
    list: Prop<Rc<[T]>>,
    key_fn: Box<dyn FnMut(&T) -> K>,
    node_fn: Box<dyn FnMut(&T) -> Node>,
    animation: PresenceAnimation,
    transition: Transition,
    move_transition: Transition,
    style: Style,
}

/// Map a list to keyed nodes and animate entering, leaving, and moving items.
pub fn transition_group<T, K, N>(
    list: impl Into<Prop<Rc<[T]>>>,
    key_fn: impl FnMut(&T) -> K + 'static,
    mut node_fn: impl FnMut(&T) -> N + 'static,
) -> TransitionGroupBuilder<T, K>
where
    T: 'static,
    K: Hash + Eq + Clone + 'static,
    N: IntoNode,
{
    TransitionGroupBuilder {
        list: list.into(),
        key_fn: Box::new(key_fn),
        node_fn: Box::new(move |value| node_fn(value).into_node()),
        animation: PresenceAnimation::fade().scale(0.96),
        transition: Transition::ease_out(0.22),
        move_transition: Transition::ease_out(0.25),
        style: Style::new(),
    }
}

impl<T, K> TransitionGroupBuilder<T, K> {
    /// Enter and leave effect of each item.
    pub fn animation(mut self, animation: PresenceAnimation) -> Self {
        self.animation = animation;
        self
    }

    /// Curve and duration of entering and leaving.
    pub fn transition(mut self, transition: Transition) -> Self {
        self.transition = transition;
        self
    }

    /// Curve and duration of moves; `Transition::Instant` disables them.
    pub fn move_transition(mut self, transition: Transition) -> Self {
        self.move_transition = transition;
        self
    }
}

impl<T, K> Styleable for TransitionGroupBuilder<T, K> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Entering(f32),
    Present,
    Leaving(f32),
}

struct Item {
    node: Node,
    /// Owns the item's resources; dropped after the node.
    _owner: Option<OwnedScope>,
    handle: NodeHandle,
    phase: Phase,
    /// 0 hidden .. 1 shown.
    progress: Signal<f32>,
    /// Absolute placement while leaving (relative to the group).
    pinned: Signal<Option<Rect>>,
    /// FLIP offset and its animation.
    shift: Signal<Vector>,
    shift_from: Vector,
    shift_time: f32,
    /// Last layout position relative to the group.
    last: Option<Rect>,
}

struct State<K> {
    order: Vec<K>,
    items: HashMap<K, Item>,
    /// Leaving items, kept until their animation ends.
    leaving: Vec<K>,
    initialized: bool,
}

impl<T, K> BuildNode for TransitionGroupBuilder<T, K>
where
    T: 'static,
    K: Hash + Eq + Clone + 'static,
{
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            list,
            mut key_fn,
            mut node_fn,
            animation,
            transition,
            move_transition,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(Style::new().flex_column().relative(), move |style| {
            let owner = current_scope();
            let state: Rc<RefCell<State<K>>> = Rc::new(RefCell::new(State {
                order: Vec::new(),
                items: HashMap::new(),
                leaving: Vec::new(),
                initialized: false,
            }));
            let refs: Signal<Rc<[NodeRef]>> = signal(Rc::from([]));

            // Reconcile the list: create entering items, mark leaving ones.
            {
                let state = state.clone();
                watch_effect(move || {
                    let values = list.get_clone();
                    untracked(|| {
                        let mut state = state.borrow_mut();
                        let initial = !state.initialized;
                        state.initialized = true;
                        let keys: Vec<K> = values.iter().map(&mut key_fn).collect();
                        for (value, key) in values.iter().zip(&keys) {
                            if let Some(item) = state.items.get_mut(key) {
                                if let Phase::Leaving(t) = item.phase {
                                    // Re-added while leaving: come back.
                                    item.phase = Phase::Entering(1. - t);
                                    item.pinned.set(None);
                                }
                                continue;
                            }
                            let item = owner.run(|| {
                                let scope = OwnedScope::new_inherited();
                                let item = scope.run(|| {
                                    build_item(
                                        node_fn(value),
                                        animation,
                                        if initial { 1. } else { 0. },
                                    )
                                });
                                Item {
                                    _owner: Some(scope),
                                    ..item
                                }
                            });
                            let item = Item {
                                phase: if initial {
                                    Phase::Present
                                } else {
                                    Phase::Entering(0.)
                                },
                                ..item
                            };
                            state.items.insert(key.clone(), item);
                        }
                        let removed: Vec<K> = state
                            .order
                            .iter()
                            .filter(|key| !keys.contains(key))
                            .cloned()
                            .collect();
                        for key in removed {
                            if let Some(item) = state.items.get_mut(&key)
                                && !matches!(item.phase, Phase::Leaving(_))
                            {
                                let t = match item.phase {
                                    Phase::Entering(t) => 1. - t,
                                    _ => 0.,
                                };
                                item.phase = Phase::Leaving(t);
                                // Pin it where it was so the others can move in.
                                item.pinned.set(item.last.or(Some(Rect::default())));
                                if !state.leaving.contains(&key) {
                                    state.leaving.push(key);
                                }
                            }
                        }
                        state.leaving.retain(|key| !keys.contains(key));
                        state.order = keys;
                        refs.set(collect_refs(&state));
                    });
                });
            }
            set_children(computed(move |_| refs.get_clone()).owned());
            use_container(style);
            let group_rect = crate::node::use_rect();

            // FLIP: after layout, start moves for items whose place changed.
            style.on_layout.append({
                let state = state.clone();
                move |_| {
                    let origin = group_rect.get_untracked();
                    let mut state = state.borrow_mut();
                    for item in state.items.values_mut() {
                        if matches!(item.phase, Phase::Leaving(_)) {
                            continue;
                        }
                        let Some(rect) = item.handle.rect() else {
                            continue;
                        };
                        let relative =
                            Rect::new(rect.x - origin.x, rect.y - origin.y, rect.w, rect.h);
                        if let Some(last) = item.last
                            && (last.x != relative.x || last.y != relative.y)
                            && move_transition.duration() > 0.
                            && item.phase == Phase::Present
                        {
                            // Start from where it is drawn now (mid-move included).
                            let current = item.shift.get_untracked();
                            item.shift_from =
                                Vector::new(last.x - relative.x, last.y - relative.y) + current;
                            item.shift_time = 0.;
                            item.shift.set(item.shift_from);
                        }
                        item.last = Some(relative);
                    }
                }
            });

            // Advance enter, leave, and move animations.
            style.on_update.append({
                let state = state.clone();
                move |dt: &f32| {
                    let dt = dt.clamp(0., 0.05);
                    let mut finished = Vec::new();
                    {
                        let mut state = state.borrow_mut();
                        let duration = transition.duration().max(0.001);
                        for (key, item) in state.items.iter_mut() {
                            match item.phase {
                                Phase::Entering(t) => {
                                    let t = (t + dt / duration).min(1.);
                                    item.progress.set(transition.sample(t));
                                    item.phase = if t >= 1. {
                                        Phase::Present
                                    } else {
                                        Phase::Entering(t)
                                    };
                                }
                                Phase::Leaving(t) => {
                                    let t = (t + dt / duration).min(1.);
                                    item.progress.set(1. - transition.sample(t));
                                    item.phase = Phase::Leaving(t);
                                    if t >= 1. {
                                        finished.push(key.clone());
                                    }
                                }
                                Phase::Present => {}
                            }
                            if item.shift_from != Vector::ZERO {
                                let move_duration = move_transition.duration().max(0.001);
                                item.shift_time = (item.shift_time + dt / move_duration).min(1.);
                                let remaining = 1. - move_transition.sample(item.shift_time);
                                item.shift.set(item.shift_from * remaining);
                                if item.shift_time >= 1. {
                                    item.shift_from = Vector::ZERO;
                                    item.shift.set(Vector::ZERO);
                                }
                            }
                        }
                    }
                    if !finished.is_empty() {
                        let removed: Vec<Item> = {
                            let mut state = state.borrow_mut();
                            state.leaving.retain(|key| !finished.contains(key));
                            let removed = finished
                                .iter()
                                .filter_map(|key| state.items.remove(key))
                                .collect();
                            refs.set(collect_refs(&state));
                            removed
                        };
                        // Dispose nodes after the children no longer list them.
                        drop(removed);
                    }
                }
            });
        })
    }
}

/// Present items in list order, then leaving ones (pinned, out of the flow).
fn collect_refs<K: Hash + Eq>(state: &State<K>) -> Rc<[NodeRef]> {
    state
        .order
        .iter()
        .chain(&state.leaving)
        .filter_map(|key| state.items.get(key))
        .map(|item| *item.node)
        .collect()
}

fn build_item(content: Node, animation: PresenceAnimation, initial: f32) -> Item {
    let progress = signal(initial);
    let pinned: Signal<Option<Rect>> = signal(None);
    let shift = signal(Vector::ZERO);
    let handle = NodeHandle::new();
    // A column so the item fills the wrapper's width, as it would unwrapped.
    let mut wrapper = flexbox()
        .flex_column()
        .node_handle(handle)
        .position(pinned.map(|pinned| {
            if pinned.is_some() {
                Position::Absolute
            } else {
                Position::Relative
            }
        }))
        .inset_left(pinned.map(|pinned| pinned.map_or(Size::Auto, |rect| Size::Px(rect.x))))
        .inset_top(pinned.map(|pinned| pinned.map_or(Size::Auto, |rect| Size::Px(rect.y))))
        .width(pinned.map(|pinned| pinned.map_or(Size::Auto, |rect| Size::Px(rect.w))))
        .transform(computed(move |_| {
            let p = progress.get().clamp(0., 1.);
            let hidden = 1. - p;
            let shift = shift.get();
            let mut ops = vec![TransformOp::Translate(
                Size::Px(shift.x + animation.offset.0 * hidden),
                Size::Px(shift.y + animation.offset.1 * hidden),
            )];
            if animation.scale != 1. {
                let scale = animation.scale + (1. - animation.scale) * p;
                ops.push(TransformOp::Scale(scale, scale));
            }
            Transform(ops)
        }));
    if animation.fade {
        wrapper = wrapper.opacity(computed(move |_| progress.get().clamp(0., 1.)));
    }
    let node = wrapper.children(content.into_children()).into_node();
    Item {
        node,
        _owner: None,
        handle,
        phase: Phase::Present,
        progress,
        pinned,
        shift,
        shift_from: Vector::ZERO,
        shift_time: 0.,
        last: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::text;

    #[test]
    fn removed_items_stay_until_their_exit_finishes() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let list: Signal<Rc<[u32]>> = signal(Rc::from([1, 2, 3]));
            let root = transition_group(list, |n| *n, |n| text(Rc::<str>::from(n.to_string())))
                .into_node();
            root.prepare_layout(Vector::new(200., 200.));
            assert_eq!(root.children().get_clone_untracked().len(), 3);
            list.set(Rc::from([1, 3]));
            assert_eq!(
                root.children().get_clone_untracked().len(),
                3,
                "the removed item animates out first"
            );
            list.set(Rc::from([1, 3, 4]));
            assert_eq!(root.children().get_clone_untracked().len(), 4);
        });
    }
}
