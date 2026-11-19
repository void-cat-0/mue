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
