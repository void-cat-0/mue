use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hash,
    mem,
    rc::Rc,
};

use mue_core::{
    Disposable, Owned,
    effect::{computed, computed_always, untracked},
    prelude::current_scope,
    prop::Prop,
    scope::OwnedScope,
    signal::{Access, ReadSignal, signal},
};

use crate::node::{IntoNode, Node, NodeRef};

/// A reactive ordered list of borrowed node handles.
pub type Children = ReadSignal<Rc<[NodeRef]>>;

/// Reactive data exposed to a retained row created by `map_keyed_prop`.
#[derive(Clone)]
pub struct KeyedItem<K, T> {
    pub key: K,
    pub value: Prop<T>,
    pub index: Prop<usize>,
}

struct KeyedSnapshot<K, T> {
    values: Rc<[T]>,
    keys: Vec<K>,
    indices: HashMap<K, usize>,
}

struct KeyedPropState<K, T> {
    // Field order keeps row consumers and projections alive until after cleanup,
    // and keeps the shared snapshot alive until all rows have been disposed.
    nodes: HashMap<K, (Node, OwnedScope)>,
    _snapshot_owner: OwnedScope,
    last_values: Rc<RefCell<HashMap<K, (T, usize)>>>,
}

impl<K, T> Drop for KeyedPropState<K, T> {
    fn drop(&mut self) {
        untracked(|| self.nodes.clear());
    }
}

/// A one-shot, deferred conversion to an owned child list.
pub struct ChildrenPlan(Box<dyn FnOnce() -> Owned<Children>>);

impl ChildrenPlan {
    /// Defer child construction until the parent is ready to build its children.
    pub fn new(build: impl FnOnce() -> Owned<Children> + 'static) -> Self {
        Self(Box::new(build))
    }
}

/// Convert nodes, tuples, and reactive child lists into owned children.
pub trait IntoChildren: 'static {
    fn into_children(self) -> Owned<Children>;

    /// Defer conversion until the parent mounts its children.
    fn into_children_plan(self) -> ChildrenPlan
    where
        Self: Sized,
    {
        ChildrenPlan::new(move || self.into_children())
    }
}

impl IntoChildren for ChildrenPlan {
    fn into_children(self) -> Owned<Children> {
        (self.0)()
    }

    fn into_children_plan(self) -> ChildrenPlan {
        self
    }
}

impl IntoChildren for () {
    fn into_children(self) -> Owned<Children> {
        (*signal(Rc::default())).owned()
    }
}

impl IntoChildren for Owned<Children> {
    fn into_children(self) -> Owned<Children> {
        self
    }
}

impl<T: IntoNode + 'static> IntoChildren for T {
    fn into_children(self) -> Owned<Children> {
        let node = self.into_node();
        computed(move |_| {
            let _ = &node;
            Rc::from([*node])
        })
        .owned()
    }
}

impl<T: IntoChildren> IntoChildren for Vec<T> {
    fn into_children(self) -> Owned<Children> {
        let children = self.into_iter().map(IntoChildren::into_children).collect();
        join_owned_children(children)
    }
}

impl<T: IntoChildren, const N: usize> IntoChildren for [T; N] {
    fn into_children(self) -> Owned<Children> {
        let children = self.into_iter().map(IntoChildren::into_children).collect();
        join_owned_children(children)
    }
}

impl IntoChildren for Rc<[Node]> {
    fn into_children(self) -> Owned<Children> {
        computed(move |_| {
            let refs: Rc<[NodeRef]> = self.iter().map(|node| **node).collect();
            refs
        })
        .owned()
    }
}

fn join_owned_children(children: Rc<[Owned<Children>]>) -> Owned<Children> {
    let mut buffer = Vec::with_capacity(children.len());
    computed(move |_| {
        buffer.clear();
        buffer.extend(children.iter().map(|child| child.get_clone()));
        buffer
            .iter()
            .flat_map(|children| children.iter().copied())
            .collect()
    })
    .owned()
}

impl IntoChildren for Rc<[Owned<Children>]> {
    fn into_children(self) -> Owned<Children> {
        join_owned_children(self)
    }
}

/// Defer flattening nodes or child lists, preserving their order.
///
/// Tuples can combine different builders, nodes, plans, and existing child lists.
/// Vectors and arrays can contain any one of those child types.
pub fn join_children(children: impl IntoChildren) -> ChildrenPlan {
    children.into_children_plan()
}

/// Map a reactive list to nodes, retaining each node while its key is present.
/// Keys must be unique; retained nodes keep their original content and local state.
pub fn map_keyed<T, K, N>(
    list: impl Into<Prop<Rc<[T]>>>,
    mut key_fn: impl FnMut(&T) -> K + 'static,
    mut node_fn: impl FnMut(&T) -> N + 'static,
) -> ChildrenPlan
where
    T: 'static,
    K: Hash + Eq + 'static,
    N: IntoNode + 'static,
{
    let list = list.into();
    ChildrenPlan::new(move || {
        let owner_scope = current_scope();
        // Own callback-created resources for exactly as long as their keyed node.
        // Node is first so its own consumers are dropped before this extra scope.
        let mut node_cache: HashMap<K, (Node, OwnedScope)> = HashMap::new();

        computed(move |_| {
            let list = list.get_clone();
            let keys: Vec<K> = list.iter().map(&mut key_fn).collect();
            let mut seen = HashSet::with_capacity(keys.len());
            for (index, key) in keys.iter().enumerate() {
                assert!(
                    seen.insert(key),
                    "map_keyed: duplicate key at index {index}; keys must be unique"
                );
            }

            let mut new_nodes = Vec::with_capacity(list.len());
            let mut previous = mem::take(&mut node_cache);
            for (value, key) in list.iter().zip(keys) {
                let entry = previous.remove(&key).unwrap_or_else(|| {
                    owner_scope.run(|| {
                        let scope = OwnedScope::new_inherited();
                        let node = scope.run(|| untracked(|| node_fn(value).into_node()));
                        (node, scope)
                    })
                });
                new_nodes.push(*entry.0);
                node_cache.insert(key, entry);
            }
            let children = new_nodes.into();
            untracked(|| drop(previous));
            children
        })
        .owned()
    })
}

/// Map a reactive list while exposing each retained row's latest item and index.
///
/// A retained key keeps its node and local state. Value and index bindings read the
/// same list snapshot, including when pulled before an outer batch finishes.
/// Removing a key disposes its node and bindings; its cleanup can read the last
/// value and index observed by the shared snapshot. Unobserved writes in a batch
/// may be coalesced. Construction reads are untracked, as with map_keyed.
pub fn map_keyed_prop<T, K, N>(
    list: impl Into<Prop<Rc<[T]>>>,
    mut key_fn: impl FnMut(&T) -> K + 'static,
    mut node_fn: impl FnMut(KeyedItem<K, T>) -> N + 'static,
) -> ChildrenPlan
where
    T: Clone + PartialEq + 'static,
    K: Clone + Hash + Eq + 'static,
    N: IntoNode + 'static,
{
    let list = list.into();
    ChildrenPlan::new(move || {
        let owner_scope = current_scope();
        let snapshot_owner = OwnedScope::new_inherited();
        let last_values = Rc::new(RefCell::new(HashMap::<K, (T, usize)>::new()));
        let last_for_snapshot = last_values.clone();
        let rows = snapshot_owner.run(|| {
            computed_always(move |previous: Option<&Rc<KeyedSnapshot<K, T>>>| {
                let Some(values) = list.try_get_clone() else {
                    // Parent scopes may dispose an input before the children output.
                    // Row teardown still needs its last successfully read snapshot.
                    return previous
                        .expect("keyed list source unavailable at mount")
                        .clone();
                };
                let mut keys = Vec::with_capacity(values.len());
                let mut indices = HashMap::with_capacity(values.len());
                for (index, value) in values.iter().enumerate() {
                    let key = key_fn(value);
                    assert!(
                        indices.insert(key.clone(), index).is_none(),
                        "map_keyed_prop: duplicate key at index {index}; keys must be unique"
                    );
                    keys.push(key);
                }
                // Keep removed rows' last active pair until their cleanup finishes,
                // even across several snapshot pulls before children are reconciled.
                untracked(|| {
                    let mut last = last_for_snapshot.borrow_mut();
                    for (index, key) in keys.iter().enumerate() {
                        last.insert(key.clone(), (values[index].clone(), index));
                    }
                });
                Rc::new(KeyedSnapshot {
                    values,
                    keys,
                    indices,
                })
            })
        });
        let mut state = KeyedPropState {
            nodes: HashMap::new(),
            _snapshot_owner: snapshot_owner,
            last_values,
        };
        computed(move |_| {
            let snapshot = rows.get_clone();
            let mut previous = mem::take(&mut state.nodes);
            let mut refs = Vec::with_capacity(snapshot.keys.len());
            for key in &snapshot.keys {
                let entry = previous.remove(key).unwrap_or_else(|| {
                    owner_scope.run(|| {
                        let scope = OwnedScope::new_inherited();
                        let node = scope.run(|| {
                            untracked(|| {
                                let pair_key = key.clone();
                                let last = state.last_values.clone();
                                let pair = computed(move |_| {
                                    let snapshot = rows.get_clone();
                                    if let Some(&index) = snapshot.indices.get(&pair_key) {
                                        (snapshot.values[index].clone(), index)
                                    } else {
                                        last.borrow()
                                            .get(&pair_key)
                                            .cloned()
                                            .expect("keyed item has no observed value")
                                    }
                                });
                                let row = KeyedItem {
                                    key: key.clone(),
                                    value: Prop::Dynamic(pair.map(|(value, _)| value)),
                                    index: Prop::Dynamic(pair.map(|(_, index)| index)),
                                };
                                node_fn(row).into_node()
                            })
                        });
                        (node, scope)
                    })
                });
                refs.push(*entry.0);
                state.nodes.insert(key.clone(), entry);
            }
            untracked(|| {
                drop(previous);
                state
                    .last_values
                    .borrow_mut()
                    .retain(|key, _| snapshot.indices.contains_key(key));
            });
            Rc::<[NodeRef]>::from(refs)
        })
        .owned()
    })
}

/// Build one subtree for a reactive value, retaining it while the value is equal.
///
/// Construction is deferred until the parent mounts its children. A changed value
/// replaces the node and disposes its callback-owned resources; switching back
/// builds a fresh subtree. Reads in `node_fn` do not subscribe this selector, but
/// nested effects and computed bindings keep their own dependencies.
pub fn keyed<T, N>(
    value: impl Into<Prop<T>>,
    mut node_fn: impl FnMut(&T) -> N + 'static,
) -> ChildrenPlan
where
    T: Clone + PartialEq + 'static,
    N: IntoNode + 'static,
{
    let value = value.into();
    ChildrenPlan::new(move || {
        let owner_scope = current_scope();
        // Dispose the node's consumers before callback-created resources.
        let mut current: Option<(Node, OwnedScope, T)> = None;
        computed(move |previous: Option<&Rc<[NodeRef]>>| {
            let value = value.get_clone();
            untracked(|| {
                if current.as_ref().is_some_and(|(_, _, key)| key == &value) {
                    return previous.unwrap().clone();
                }
                let (node, scope) = owner_scope.run(|| {
                    let scope = OwnedScope::new_inherited();
                    let node = scope.run(|| node_fn(&value).into_node());
                    (node, scope)
                });
                let refs = Rc::from([*node]);
                current = Some((node, scope, value));
                refs
            })
        })
        .owned()
    })
}

/// Show an existing or deferred node conditionally, retaining its state while hidden.
pub fn show_if(condition: impl Into<Prop<bool>>, node: impl IntoNode + 'static) -> ChildrenPlan {
    let condition = condition.into();
    ChildrenPlan::new(move || {
        let node = node.into_node();
        let refs = Rc::<[NodeRef]>::from([*node]);
        computed(move |_| {
            let _ = &node;
            if condition.get() {
                refs.clone()
            } else {
                Rc::default()
            }
        })
        .owned()
    })
}

macro_rules! impl_tuple_children {
    ($($item:ident)*) => {
        #[allow(non_snake_case)]
        impl<$($item: IntoChildren),*> IntoChildren for ($($item,)*) {
            fn into_children(self) -> Owned<Children> {
                let ($($item,)*) = self;
                let children = [$($item.into_children()),*];
                join_owned_children(children.into())
            }
        }
    };
}

impl_tuple_children!(T1);
impl_tuple_children!(T1 T2);
impl_tuple_children!(T1 T2 T3);
impl_tuple_children!(T1 T2 T3 T4);
impl_tuple_children!(T1 T2 T3 T4 T5);
impl_tuple_children!(T1 T2 T3 T4 T5 T6);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16);

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        panic::{AssertUnwindSafe, catch_unwind},
    };

    use super::*;
    use mue_core::batch;

    struct CountedNode(Rc<Cell<usize>>);

    impl IntoNode for CountedNode {
        fn into_node(self) -> Node {
            self.0.set(self.0.get() + 1);
            Node::empty()
        }
    }

    #[test]
    fn tuple_vector_and_array_plans_defer_node_construction() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let builds = Rc::new(Cell::new(0));
            let tuple = (CountedNode(builds.clone()), Node::empty()).into_children_plan();
            let vector =
                vec![CountedNode(builds.clone()), CountedNode(builds.clone())].into_children_plan();
            let array =
                [CountedNode(builds.clone()), CountedNode(builds.clone())].into_children_plan();
            assert_eq!(builds.get(), 0);

            let children = join_children((tuple, vector, array)).into_children();
            assert_eq!(builds.get(), 5);
            assert_eq!(children.get_clone().len(), 6);
        });
    }

    #[test]
    fn dropping_an_unmounted_plan_does_not_build_children() {
        let builds = Rc::new(Cell::new(0));
        let plan = CountedNode(builds.clone()).into_children_plan();
        drop(plan);
        assert_eq!(builds.get(), 0);
    }

    #[test]
    fn shared_node_container_keeps_its_owning_semantics() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let node = Node::empty();
            let expected = *node;
            let nodes: Rc<[Node]> = Rc::from([node]);
            let retained = nodes.clone();
            let plan = nodes.into_children_plan();
            assert_eq!(Rc::strong_count(&retained), 2);
            let children = plan.into_children();
            assert!(children.get_clone().as_ref() == &[expected]);
            assert_eq!(Rc::strong_count(&retained), 2);
            drop(children);
            assert_eq!(Rc::strong_count(&retained), 1);
        });
    }

    #[test]
    fn shared_owned_lists_remain_owned_until_the_shared_container_drops() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let lists: Rc<[Owned<Children>]> = Rc::from([().into_children()]);
            let source = *lists[0];
            let retained = lists.clone();
            let children = join_children(lists).into_children();
            assert!(children.get_clone().is_empty());
            assert!(!source.is_disposed());
            drop(children);
            assert!(!source.is_disposed());
            drop(retained);
            assert!(source.is_disposed());
        });
    }

    #[test]
    fn show_if_defers_construction_and_retains_the_node_while_hidden() {
        let source_owner = OwnedScope::new();
        let visible = source_owner.run(|| signal(false));
        let builds = Rc::new(Cell::new(0));
        let plan = show_if(visible, CountedNode(builds.clone()));
        assert_eq!(builds.get(), 0);
        let owner = OwnedScope::new();
        let children = owner.run(|| plan.into_children());
        assert_eq!(builds.get(), 1);
        assert!(children.get_clone().is_empty());
        visible.set(true);
        let first = children.get_clone()[0];
        visible.set(false);
        assert!(children.get_clone().is_empty());
        visible.set(true);
        assert!(children.get_clone()[0] == first);
        assert_eq!(builds.get(), 1);
        drop(children);
        drop(owner);
        assert!(!visible.is_disposed());
    }

    #[test]
    fn keyed_nodes_reuse_state_and_dispose_only_the_removed_keys_resources() {
        let source_owner = OwnedScope::new();
        let source = source_owner.run(|| signal(Rc::<[i32]>::from([1, 2])));
        let owner = OwnedScope::new();
        owner.provide(17u32);
        let resources = Rc::new(RefCell::new(Vec::new()));
        let captured = resources.clone();
        let plan = map_keyed(
            source,
            |key| *key,
            move |value| {
                assert_eq!(current_scope().inject::<u32>(), Some(17));
                let resource = signal(*value);
                captured.borrow_mut().push((*value, *resource));
                Node::empty()
            },
        );
        assert!(resources.borrow().is_empty());
        let children = owner.run(|| plan.into_children());
        let original = children.get_clone();
        assert_eq!(resources.borrow().len(), 2);

        source.set(Rc::from([2, 1]));
        let reordered = children.get_clone();
        assert!(reordered[0] == original[1]);
        assert!(reordered[1] == original[0]);
        assert_eq!(resources.borrow().len(), 2);

        source.set(Rc::from([2, 3]));
        let updated = children.get_clone();
        assert!(updated[0] == original[1]);
        assert_eq!(resources.borrow().len(), 3);
        assert!(resources.borrow()[0].1.is_disposed());
        assert!(!resources.borrow()[1].1.is_disposed());
        assert!(!resources.borrow()[2].1.is_disposed());

        drop(children);
        assert!(
            resources
                .borrow()
                .iter()
                .all(|(_, signal)| signal.is_disposed())
        );
        drop(owner);
        assert!(!source.is_disposed());
        source.set(Rc::from([4]));
        assert_eq!(source.get_clone().as_ref(), &[4]);
    }

    #[test]
    fn later_keys_inherit_the_mount_context_and_dispose_removed_keys() {
        let source_owner = OwnedScope::new();
        let source = source_owner.run(|| signal(Rc::<[i32]>::from([])));
        let owner = OwnedScope::new();
        owner.provide(23u32);
        let resources = Rc::new(RefCell::new(Vec::new()));
        let captured = resources.clone();
        let plan = map_keyed(
            source,
            |key| *key,
            move |value| {
                assert_eq!(current_scope().inject::<u32>(), Some(23));
                let resource = signal(*value);
                captured.borrow_mut().push(*resource);
                Node::empty()
            },
        );
        let children = owner.run(|| plan.into_children());
        source.set(Rc::from([1]));
        assert_eq!(children.get_clone().len(), 1);
        assert!(!resources.borrow()[0].is_disposed());

        // A later key is built against the mount context, not the updating one.
        let update_scope = OwnedScope::new();
        update_scope.provide(99u32);
        update_scope.run(|| source.set(Rc::from([1, 2])));
        assert_eq!(children.get_clone().len(), 2);
        assert!(!resources.borrow()[0].is_disposed());
        assert!(!resources.borrow()[1].is_disposed());

        update_scope.run(|| source.set(Rc::from([2])));
        assert_eq!(children.get_clone().len(), 1);
        assert!(resources.borrow()[0].is_disposed());
        assert!(!resources.borrow()[1].is_disposed());

        drop(children);
        drop(owner);
        assert!(resources.borrow()[1].is_disposed());
        assert!(!source.is_disposed());
    }

    #[test]
    fn map_keyed_node_reads_do_not_become_list_dependencies() {
        let owner = OwnedScope::new();
        let list = owner.run(|| signal(Rc::<[i32]>::from([1])));
        let snapshot = owner.run(|| signal(10));
        let snapshot_read: ReadSignal<i32> = *snapshot;
        let key_calls = Rc::new(Cell::new(0));
        let counted_keys = key_calls.clone();
        let builds = Rc::new(Cell::new(0));
        let counted_builds = builds.clone();
        let children = owner.run(|| {
            map_keyed(
                list,
                move |key| {
                    counted_keys.set(counted_keys.get() + 1);
                    *key
                },
                move |_| {
                    snapshot_read.get();
                    CountedNode(counted_builds.clone())
                },
            )
            .into_children()
        });
        assert_eq!(key_calls.get(), 1);
        assert_eq!(builds.get(), 1);
        snapshot.set(11);
        assert_eq!(key_calls.get(), 1);
        assert_eq!(builds.get(), 1);
        list.set(Rc::from([2]));
        assert_eq!(key_calls.get(), 2);
        assert_eq!(builds.get(), 2);
        assert_eq!(children.get_clone().len(), 1);
    }

    #[test]
    fn keyed_props_update_retained_values_indices_and_event_reads_without_rebuilding() {
        use mue_core::effect::watch_effect;

        #[derive(Clone, PartialEq, Debug)]
        struct Item {
            id: u32,
            label: String,
        }
        let make = |id, label: &str| Item {
            id,
            label: label.into(),
        };
        let owner = OwnedScope::new();
        let source = owner.run(|| signal(Rc::from([make(1, "a"), make(2, "b")])));
        let builds = Rc::new(Cell::new(0));
        let built = builds.clone();
        let handles = Rc::new(RefCell::new(HashMap::new()));
        let saved = handles.clone();
        let events = Rc::new(RefCell::new(HashMap::<u32, Box<dyn Fn() -> String>>::new()));
        let saved_events = events.clone();
        let value_runs = Rc::new(Cell::new(0));
        let observed_values = value_runs.clone();
        let index_runs = Rc::new(Cell::new(0));
        let observed_indices = index_runs.clone();
        let plan = map_keyed_prop(
            source,
            |item: &Item| item.id,
            move |row| {
                built.set(built.get() + 1);
                let KeyedItem { key, value, index } = row;
                saved.borrow_mut().insert(key, (value.clone(), index));
                let live_value = value.clone();
                saved_events
                    .borrow_mut()
                    .insert(key, Box::new(move || live_value.get_clone().label));
                let runs = observed_values.clone();
                let observed_value = value.clone();
                watch_effect(move || {
                    observed_value.get_clone();
                    runs.set(runs.get() + 1);
                });
                let runs = observed_indices.clone();
                watch_effect(move || {
                    index.get();
                    runs.set(runs.get() + 1);
                });
                Node::empty()
            },
        );
        assert_eq!(builds.get(), 0);
        let children = owner.run(|| plan.into_children());
        let initial = children.get_clone();
        let revision = children.revision_untracked();
        assert_eq!(builds.get(), 2);
        source.set(Rc::from([make(1, "A"), make(2, "b")]));
        assert!(children.get_clone().as_ref() == initial.as_ref());
        assert_eq!(children.revision_untracked(), revision);
        assert_eq!(builds.get(), 2);
        assert_eq!(events.borrow()[&1](), "A");
        assert_eq!(value_runs.get(), 3);
        assert_eq!(index_runs.get(), 2);

        source.set(Rc::from([make(2, "b"), make(1, "A")]));
        let reordered = children.get_clone();
        assert!(reordered[0] == initial[1] && reordered[1] == initial[0]);
        assert_eq!(handles.borrow()[&1].1.get(), 1);
        assert_eq!(handles.borrow()[&2].1.get(), 0);
        assert_eq!(builds.get(), 2);
        assert_eq!(value_runs.get(), 3);
        assert_eq!(index_runs.get(), 4);

        source.set(Rc::from([make(3, "c"), make(2, "b"), make(1, "A")]));
        assert_eq!(builds.get(), 3);
        assert_eq!(handles.borrow()[&1].1.get(), 2);
        assert!(children.get_clone()[1] == initial[1]);
        assert!(children.get_clone()[2] == initial[0]);
        source.set(Rc::from([make(3, "c"), make(2, "b")]));
        assert!(handles.borrow()[&1].0.try_get_clone().is_none());
        source.set(Rc::from([make(1, "new"), make(3, "c"), make(2, "b")]));
        assert_eq!(builds.get(), 4);
        assert!(children.get_clone()[0] != initial[0]);
        assert_eq!(events.borrow()[&1](), "new");
        drop(children);
        assert!(
            handles
                .borrow()
                .values()
                .all(|(value, index)| value.try_get_clone().is_none() && index.try_get().is_none())
        );
        source.set(Rc::from([make(4, "d")]));
        assert_eq!(builds.get(), 4);
    }

    #[test]
    fn keyed_props_keep_last_observed_values_for_cleanup() {
        use mue_core::{
            effect::{on_cleanup, watch_effect},
            runtime::resource_counts,
        };

        let source_owner = OwnedScope::new();
        let source = source_owner.run(|| signal(Rc::from([(1, 10), (2, 20)])));
        let owner = OwnedScope::new();
        owner.provide(17u32);
        let cleanups = Rc::new(RefCell::new(Vec::new()));
        let cleaned = cleanups.clone();
        let handles = Rc::new(RefCell::new(HashMap::new()));
        let saved = handles.clone();
        let baseline = resource_counts();
        let plan = map_keyed_prop(
            source,
            |item| item.0,
            move |row| {
                assert_eq!(current_scope().inject::<u32>(), Some(17));
                let KeyedItem { key, value, index } = row;
                saved.borrow_mut().insert(key, (value, index));
                let cleaned = cleaned.clone();
                watch_effect(move || {
                    let cleaned = cleaned.clone();
                    on_cleanup(move || {
                        cleaned.borrow_mut().push((key, value.get().1, index.get()));
                    });
                });
                Node::empty()
            },
        );
        let children = owner.run(|| plan.into_children());
        source.set(Rc::from([(2, 21), (1, 11)]));
        let update_scope = OwnedScope::new();
        update_scope.provide(99u32);
        update_scope.run(|| source.set(Rc::from([(1, 12), (3, 30)])));
        assert_eq!(&*cleanups.borrow(), &[(2, 21, 0)]);
        // Unobserved writes in a batch are coalesced. The removed row keeps 12.
        batch(|| {
            source.set(Rc::from([(1, 99), (3, 30)]));
            source.set(Rc::from([(3, 31)]));
        });
        assert_eq!(cleanups.borrow()[1], (1, 12, 0));
        // Explicitly pulling the row advances the shared snapshot, even before
        // children reconcile, and must survive several later snapshot versions.
        let (value, index) = handles.borrow()[&3];
        batch(|| {
            source.set(Rc::from([(4, 40), (3, 32)]));
            assert_eq!(value.get(), (3, 32));
            assert_eq!(index.get(), 1);
            source.set(Rc::from([]));
            assert_eq!(value.get(), (3, 32));
            source.set(Rc::from([(5, 50)]));
            assert_eq!(value.get(), (3, 32));
        });
        assert_eq!(cleanups.borrow()[2], (3, 32, 1));
        drop(children);
        drop(update_scope);
        assert_eq!(resource_counts(), baseline);
        assert!(!source.is_disposed());
    }

    #[test]
    fn keyed_props_refresh_in_batches_and_observers_read_one_snapshot() {
        use mue_core::effect::watch_effect;

        let owner = OwnedScope::new();
        let source = owner.run(|| signal(Rc::from([(1, 10)])));
        let tick = owner.run(|| signal(0));
        let handle = Rc::new(Cell::new(None));
        let saved = handle.clone();
        let children = owner.run(|| {
            map_keyed_prop(
                source,
                |item| item.0,
                move |row| {
                    if row.key == 1 {
                        saved.set(Some((row.value, row.index)));
                    }
                    Node::empty()
                },
            )
            .into_children()
        });
        let (value, index) = handle.get().unwrap();
        let output = *children;
        let observations = Rc::new(RefCell::new(Vec::new()));
        let observed = observations.clone();
        owner.run(|| {
            watch_effect(move || {
                let tick = tick.get();
                let list = source.get_clone();
                let current_value = value.get();
                let current_index = index.get();
                let len = output.get_clone().len();
                assert_eq!(list[current_index], current_value);
                assert_eq!(len, list.len());
                observed.borrow_mut().push(tick);
            })
        });
        batch(|| {
            tick.set(1); // Queue the external observer before the list changes.
            source.set(Rc::from([(2, 20), (1, 11)]));
            assert_eq!(value.get(), (1, 11));
            assert_eq!(index.get(), 1);
            assert_eq!(children.get_clone().len(), 2);
        });
        assert_eq!(&*observations.borrow(), &[0, 1]);
        source.set(Rc::from([(1, 12), (2, 21)]));
        assert_eq!(value.get(), (1, 12));
        assert_eq!(index.get(), 0);
        drop(owner);
        assert!(value.try_get().is_none());
        assert!(children.is_disposed());
    }

    #[test]
    fn keyed_props_cleanup_survives_owner_disposal_with_a_dirty_source() {
        use mue_core::effect::{on_cleanup, watch_effect};
        let owner = OwnedScope::new();
        let source = owner.run(|| signal(Rc::from([(1, 10)])));
        let cleaned = Rc::new(Cell::new(None));
        let saved = cleaned.clone();
        let children = owner.run(|| {
            map_keyed_prop(
                source,
                |item| item.0,
                move |row| {
                    let saved = saved.clone();
                    watch_effect(move || {
                        let saved = saved.clone();
                        let value = row.value;
                        let index = row.index;
                        on_cleanup(move || saved.set(Some((value.get(), index.get()))));
                    });
                    Node::empty()
                },
            )
            .into_children()
        });
        batch(|| {
            source.set(Rc::from([(1, 11)]));
            drop(owner);
        });
        assert_eq!(cleaned.get(), Some(((1, 10), 0)));
        assert!(children.is_disposed());
    }

    #[test]
    fn keyed_props_cleanup_survives_disposal_of_a_dirty_derived_source() {
        use mue_core::effect::{on_cleanup, watch_effect};
        let owner = OwnedScope::new();
        let source = owner.run(|| signal(Rc::from([(1, 10)])));
        let derived = owner.run(|| source.map(|items| items));
        let cleaned = Rc::new(Cell::new(None));
        let saved = cleaned.clone();
        let children = owner.run(|| {
            map_keyed_prop(
                derived,
                |item| item.0,
                move |row| {
                    let saved = saved.clone();
                    watch_effect(move || {
                        let saved = saved.clone();
                        let value = row.value;
                        let index = row.index;
                        on_cleanup(move || saved.set(Some((value.get(), index.get()))));
                    });
                    Node::empty()
                },
            )
            .into_children()
        });
        batch(|| {
            source.set(Rc::from([(1, 11)]));
            drop(owner);
        });
        assert_eq!(cleaned.get(), Some(((1, 10), 0)));
        assert!(children.is_disposed());
    }

    #[test]
    fn keyed_props_reject_duplicate_keys_before_constructing_nodes() {
        let builds = Rc::new(Cell::new(0));
        let built = builds.clone();
        let plan = map_keyed_prop(
            Prop::Static(Rc::from([(1, 10), (1, 20)])),
            |item| item.0,
            move |_| CountedNode(built.clone()),
        );
        let result = catch_unwind(AssertUnwindSafe(|| plan.into_children()));
        assert!(result.is_err());
        assert_eq!(builds.get(), 0);
    }

    #[test]
    fn scalar_keyed_defers_reuses_and_replaces_with_the_mount_context() {
        use mue_core::{
            effect::{on_cleanup, watch_effect},
            runtime::resource_counts,
            signal::Signal,
        };

        let source_owner = OwnedScope::new();
        // Equal notifications must still retain the subtree; no Hash or Eq bound is needed.
        #[derive(Clone, PartialEq)]
        struct Key(i32);
        let source = source_owner.run(|| Signal::new_always(Key(1)));
        let owner = OwnedScope::new();
        owner.provide(17u32);
        let resources = Rc::new(RefCell::new(Vec::new()));
        let captured = resources.clone();
        let cleanups = Rc::new(RefCell::new(Vec::new()));
        let cleaned = cleanups.clone();
        let plan = keyed(source, move |key| {
            assert_eq!(current_scope().inject::<u32>(), Some(17));
            let resource = signal(key.0);
            captured.borrow_mut().push(*resource);
            let callback_cleaned = cleaned.clone();
            let key = key.0;
            watch_effect(move || {
                let cleaned = callback_cleaned.clone();
                on_cleanup(move || cleaned.borrow_mut().push(("callback", key)));
            });
            let cleaned = cleaned.clone();
            Node::leaf(crate::style::Style::new(), move |_| {
                watch_effect(move || {
                    let cleaned = cleaned.clone();
                    on_cleanup(move || {
                        assert!(!resource.is_disposed());
                        cleaned.borrow_mut().push(("node", key));
                    });
                });
            })
        });
        assert!(resources.borrow().is_empty());
        let baseline = resource_counts();
        let children = owner.run(|| plan.into_children());
        let initial = children.get_clone();
        let revision = children.revision_untracked();
        source.set(Key(1));
        assert!(Rc::ptr_eq(&children.get_clone(), &initial));
        assert_eq!(children.revision_untracked(), revision);
        assert_eq!(resources.borrow().len(), 1);

        let update_scope = OwnedScope::new();
        update_scope.provide(99u32);
        update_scope.run(|| source.set(Key(2)));
        assert!(children.get_clone()[0] != initial[0]);
        assert!(resources.borrow()[0].is_disposed());
        assert!(!resources.borrow()[1].is_disposed());
        source.set(Key(1));
        assert!(children.get_clone()[0] != initial[0]);
        assert_eq!(resources.borrow().len(), 3);
        assert!(resources.borrow()[1].is_disposed());

        drop(children);
        drop(update_scope);
        assert!(
            resources
                .borrow()
                .iter()
                .all(|resource| resource.is_disposed())
        );
        assert_eq!(resource_counts(), baseline);
        assert_eq!(
            &*cleanups.borrow(),
            &[
                ("node", 1),
                ("callback", 1),
                ("node", 2),
                ("callback", 2),
                ("node", 1),
                ("callback", 1),
            ]
        );
        assert!(!source.is_disposed());
        drop(owner);
    }

    #[test]
    fn scalar_keyed_isolates_snapshot_reads_but_keeps_nested_bindings() {
        use mue_core::effect::{on_cleanup, watch_effect};

        // Count comparisons to detect selector reruns even when it reuses the node.
        #[derive(Clone)]
        struct Key(i32, Rc<Cell<usize>>);
        impl PartialEq for Key {
            fn eq(&self, other: &Self) -> bool {
                self.1.set(self.1.get() + 1);
                self.0 == other.0
            }
        }
        let owner = OwnedScope::new();
        let comparisons = Rc::new(Cell::new(0));
        let source = owner.run(|| signal(Key(1, comparisons.clone())));
        let snapshot = owner.run(|| signal(10));
        let live = owner.run(|| signal(20));
        let renders = Rc::new(Cell::new(0));
        let rendered = renders.clone();
        let observed = Rc::new(Cell::new(0));
        let output = observed.clone();
        let misplaced_cleanups = Rc::new(Cell::new(0));
        let cleaned = misplaced_cleanups.clone();
        let children = owner.run(|| {
            keyed(source, move |_| {
                rendered.set(rendered.get() + 1);
                snapshot.get();
                let cleaned = cleaned.clone();
                on_cleanup(move || cleaned.set(cleaned.get() + 1));
                let output = output.clone();
                watch_effect(move || output.set(live.get()));
                Node::empty()
            })
            .into_children()
        });
        let initial = children.get_clone();
        snapshot.set(11);
        assert_eq!(
            comparisons.get(),
            0,
            "snapshot reads must not rerun the selector"
        );
        live.set(21);
        assert_eq!(observed.get(), 21);
        assert_eq!(comparisons.get(), 0);
        assert_eq!(renders.get(), 1);
        assert!(children.get_clone()[0] == initial[0]);
        source.set(Key(2, comparisons.clone()));
        assert_eq!(renders.get(), 2);
        assert_eq!(misplaced_cleanups.get(), 0);
        drop(children);
        assert_eq!(misplaced_cleanups.get(), 0);
        live.set(22);
        assert_eq!(observed.get(), 21, "removed bindings must be disposed");
    }

    #[test]
    fn scalar_keyed_accepts_static_values_and_does_not_build_dropped_plans() {
        let owner = OwnedScope::new();
        let builds = Rc::new(Cell::new(0));
        let captured = builds.clone();
        drop(keyed(Prop::Static(1), move |_| {
            CountedNode(captured.clone())
        }));
        assert_eq!(builds.get(), 0);
        let captured = builds.clone();
        let children = owner
            .run(|| keyed(Prop::Static(1), move |_| CountedNode(captured.clone())).into_children());
        assert_eq!(builds.get(), 1);
        assert_eq!(children.get_clone().len(), 1);
    }

    #[test]
    fn duplicate_keys_are_rejected_before_constructing_any_node() {
        let owner = OwnedScope::new();
        let builds = Rc::new(Cell::new(0));
        let captured = builds.clone();
        let plan = map_keyed(
            Prop::Static(Rc::<[i32]>::from([1, 1])),
            |key| *key,
            move |_| CountedNode(captured.clone()),
        );
        let result = catch_unwind(AssertUnwindSafe(|| owner.run(|| plan.into_children())));
        let panic = match result {
            Ok(_) => panic!("duplicate keys were accepted"),
            Err(panic) => panic,
        };
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(message.contains("map_keyed: duplicate key at index 1"));
        assert_eq!(builds.get(), 0);
    }
}
